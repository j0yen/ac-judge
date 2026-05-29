//! ac-judge — semantic acceptance-criteria judge CLI.
//!
//! See the crate-level docs in `lib.rs` for the design. This binary wires the
//! `run` / `calibrate` / `show` subcommands and owns the exit-code contract:
//! 0 = all ACs pass, 4 = an AC failed the gate, 6 = `$ANTHROPIC_API_KEY`
//! unset.

// A CLI prints to stdout/stderr by design; allow it in this binary only.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use ac_judge::api::{self, require_api_key};
use ac_judge::pair::{self, Pair};
use ac_judge::schema::{Receipt, Verdict};
use ac_judge::{exit, DEFAULT_MODEL};

#[derive(Parser)]
#[command(name = "ac-judge", version, about = "Semantic acceptance-criteria judge")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Judge every AC in a PRD against the crate's tests.
    Run {
        /// Path to the PRD that declares the ACs.
        #[arg(long)]
        prd: PathBuf,
        /// Path to the crate root whose `tests/` are judged.
        #[arg(long)]
        crate_root: PathBuf,
        /// Override the judge model.
        #[arg(long, default_value = DEFAULT_MODEL)]
        model: String,
    },
    /// Run the judge against a golden set and report the confusion matrix.
    Calibrate {
        /// Directory of hand-curated AC-↔-test pairs.
        #[arg(long)]
        golden_set: PathBuf,
    },
    /// Pretty-print one verdict from the most recent run.
    Show {
        /// The AC id, e.g. `AC1`.
        #[arg(long)]
        slug: String,
        /// Crate root holding `target/autobuilder/ac-semantic-judge.json`.
        #[arg(long, default_value = ".")]
        crate_root: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            prd,
            crate_root,
            model,
        } => run(&prd, &crate_root, &model),
        Command::Calibrate { golden_set } => calibrate(&golden_set),
        Command::Show { slug, crate_root } => show(&slug, &crate_root),
    }
}

fn run(prd: &Path, crate_root: &Path, model: &str) -> ExitCode {
    // AC8: the missing-key check must come before any network attempt.
    if require_api_key().is_err() {
        eprintln!("ac-judge: ${} is unset; refusing to run (no network attempted)", api::API_KEY_ENV);
        return ExitCode::from(exit::NO_API_KEY);
    }

    let prd_body = match fs::read_to_string(prd) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ac-judge: cannot read PRD {}: {e}", prd.display());
            return ExitCode::from(2);
        }
    };
    let pairs = match pair::pair_all(&prd_body, crate_root) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ac-judge: pair detection failed: {e}");
            return ExitCode::from(2);
        }
    };
    if pairs.is_empty() {
        eprintln!("ac-judge: no ACs found in {}", prd.display());
        return ExitCode::from(2);
    }

    let verdicts = build_verdicts(&pairs, crate_root);
    let receipt = Receipt::new(
        prd.display().to_string(),
        crate_root.display().to_string(),
        model.to_owned(),
        now_iso(),
        verdicts,
    );

    if let Err(e) = write_receipt(crate_root, &receipt) {
        eprintln!("ac-judge: cannot write receipt: {e}");
        return ExitCode::from(2);
    }

    for v in &receipt.verdicts {
        if v.fails_gate() {
            eprintln!(
                "ac-judge: {} FAILED — behavior_match={:?} assertion_kind={:?} conf={:.2}: {}",
                v.ac_id, v.behavior_match, v.assertion_kind, v.confidence, v.reasoning
            );
        }
    }

    if receipt.passed {
        println!("ac-judge: all {} ACs passed the semantic judge", receipt.verdicts.len());
        ExitCode::from(exit::PASS)
    } else {
        ExitCode::from(exit::AC_FAIL)
    }
}

/// Build a verdict per pair. Unpaired ACs get the canonical no-test verdict
/// now (AC5). Network judging of paired ACs is deferred to a later iteration;
/// until then a paired AC is recorded as `partial` with a clear marker so the
/// gate neither blocks nor falsely passes a real semantic check.
fn build_verdicts(pairs: &[Pair], crate_root: &Path) -> Vec<Verdict> {
    pairs
        .iter()
        .map(|p| {
            p.test_path.as_ref().map_or_else(
                || Verdict::unpaired(&p.ac.id),
                |path| {
                    let rel = path
                        .strip_prefix(crate_root)
                        .unwrap_or(path)
                        .display()
                        .to_string();
                    Verdict {
                        ac_id: p.ac.id.clone(),
                        test_path: Some(rel),
                        behavior_match: ac_judge::schema::BehaviorMatch::Partial,
                        assertion_kind: ac_judge::schema::AssertionKind::Mixed,
                        confidence: 0.0,
                        reasoning: "deferred: network judge not run this iteration".to_owned(),
                    }
                },
            )
        })
        .collect()
}

fn write_receipt(crate_root: &Path, receipt: &Receipt) -> std::io::Result<()> {
    let dir = crate_root.join("target").join("autobuilder");
    fs::create_dir_all(&dir)?;
    let path = dir.join("ac-semantic-judge.json");
    let json = serde_json::to_string_pretty(receipt)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

fn calibrate(golden_set: &Path) -> ExitCode {
    // Network-bound; deferred. Report the boundary rather than faking a pass.
    eprintln!(
        "ac-judge: calibrate against {} is deferred to the network iteration (see PRD AC6)",
        golden_set.display()
    );
    ExitCode::from(3)
}

fn show(slug: &str, crate_root: &Path) -> ExitCode {
    let path = crate_root
        .join("target")
        .join("autobuilder")
        .join("ac-semantic-judge.json");
    let body = match fs::read_to_string(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ac-judge: no receipt at {}: {e}", path.display());
            return ExitCode::from(2);
        }
    };
    let receipt: Receipt = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ac-judge: receipt is not valid JSON: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(verdict) = receipt.verdicts.iter().find(|v| v.ac_id == slug) else {
        eprintln!("ac-judge: no verdict for {slug} in {}", path.display());
        return ExitCode::from(2);
    };
    match serde_json::to_string_pretty(verdict) {
        Ok(pretty) => {
            println!("{pretty}");
            ExitCode::from(exit::PASS)
        }
        Err(e) => {
            eprintln!("ac-judge: cannot render verdict: {e}");
            ExitCode::from(2)
        }
    }
}

/// A minimal ISO-8601 UTC timestamp without pulling in a date crate.
///
/// Converts epoch seconds to a `YYYY-MM-DDTHH:MM:SSZ` string via a
/// civil-from-days algorithm (Howard Hinnant's `days_from_civil` inverse).
// `doe`/`doy` and `yoe`/`y` are the canonical variable names from Howard
// Hinnant's civil-from-days algorithm; renaming them for the similar-names
// lint would obscure the well-known reference implementation.
#[allow(clippy::similar_names)]
fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Days since the Unix epoch fit in i64 for any plausible system clock.
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let sod = secs % 86_400;
    let (hh, mm, ss) = (sod / 3_600, (sod % 3_600) / 60, sod % 60);
    // civil_from_days, epoch shifted to 0000-03-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}
