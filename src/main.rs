//! ac-judge — semantic acceptance-criteria judge CLI.
//!
//! See the crate-level docs in `lib.rs` for the design. This binary wires the
//! `run` / `calibrate` / `show` subcommands and owns the exit-code contract:
//! 0 = all ACs pass, 4 = an AC failed the gate, 6 = no judge backend
//! available (`codex login`, `$ANTHROPIC_API_KEY`, or `claude login` — all
//! three were checked and none worked, or the single explicitly requested
//! `--backend` didn't; the diagnostic on stderr says which). `--backend
//! auto|codex|api|claude-cli` (default `auto`) picks the judge's model
//! backend; `auto` prefers codex, then the Anthropic API, then the Claude
//! CLI, and never silently substitutes when a backend is named explicitly.

// A CLI prints to stdout/stderr by design; allow it in this binary only.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use ac_judge::backend::{self, Backend, Check, Requested, judge_one_ac};
use ac_judge::exit;
use ac_judge::pair::{self, Pair};
use ac_judge::schema::{Receipt, Verdict};

#[derive(Parser)]
#[command(
    name = "ac-judge",
    version,
    about = "Semantic acceptance-criteria judge"
)]
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
        /// Which judge backend to use. `auto` tries codex, then api, then
        /// claude-cli; an explicit choice never falls back.
        #[arg(long, default_value = "auto")]
        backend: String,
        /// Override the judge model. Defaults to the selected backend's own
        /// pinned default when omitted.
        #[arg(long)]
        model: Option<String>,
    },
    /// Run the judge against a golden set and report the confusion matrix.
    Calibrate {
        /// Directory of hand-curated AC-↔-test pairs.
        #[arg(long)]
        golden_set: PathBuf,
        /// Which judge backend to use. Same resolution as `run`.
        #[arg(long, default_value = "auto")]
        backend: String,
        /// Override the judge model.
        #[arg(long)]
        model: Option<String>,
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
            backend,
            model,
        } => run(&prd, &crate_root, &backend, model.as_deref()),
        Command::Calibrate {
            golden_set,
            backend,
            model,
        } => calibrate(&golden_set, &backend, model.as_deref()),
        Command::Show { slug, crate_root } => show(&slug, &crate_root),
    }
}

/// Parse `--backend` or print the parse error and return exit 2.
fn parse_backend(raw: &str) -> Result<Requested, ExitCode> {
    Requested::parse(raw).map_err(|e| {
        eprintln!("ac-judge: {e}");
        ExitCode::from(2)
    })
}

/// Print the "no judge backend available" diagnostic: one line per check.
fn print_no_backend(checks: &[Check]) {
    eprintln!("ac-judge: no judge backend available; checked:");
    for c in checks {
        eprintln!("  {c}");
    }
}

#[allow(clippy::too_many_lines)]
fn run(prd: &Path, crate_root: &Path, backend_flag: &str, model: Option<&str>) -> ExitCode {
    let requested = match parse_backend(backend_flag) {
        Ok(r) => r,
        Err(code) => return code,
    };

    // AC4/AC6: backend resolution happens before anything else — no PRD
    // read, no pairing, no directory write — so an unavailable backend never
    // costs a network call or leaves target/autobuilder/ touched.
    let backend: Box<dyn Backend> = match backend::resolve(requested, model) {
        Ok(b) => b,
        Err(checks) => {
            print_no_backend(&checks);
            return ExitCode::from(exit::NO_BACKEND);
        }
    };
    eprintln!(
        "ac-judge: backend={} model={}",
        backend.name(),
        backend.model()
    );

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
        eprintln!(
            "ac-judge: no ACs found in {} — expected `**AC<N>**: ...` bullets, \
             or numbered lines like `N. P0 — ...` under a `## Acceptance criteria` heading",
            prd.display()
        );
        return ExitCode::from(2);
    }

    let crate_name = crate_root.file_name().map_or_else(
        || "unknown".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let cache_dir = crate_root
        .join("target")
        .join("autobuilder")
        .join("ac-judge-cache");
    let verdicts = match build_verdicts(
        &pairs,
        crate_root,
        backend.as_ref(),
        &crate_name,
        &cache_dir,
    ) {
        Ok(v) => v,
        Err(msg) => {
            // AC8: a strict-JSON-violating reply is a hard failure, not a
            // conservative partial verdict — no receipt is written.
            eprintln!("ac-judge: {msg}");
            return ExitCode::from(2);
        }
    };
    let receipt = Receipt::new(
        prd.display().to_string(),
        crate_root.display().to_string(),
        backend.name(),
        backend.model(),
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
        println!(
            "ac-judge: all {} ACs passed the semantic judge",
            receipt.verdicts.len()
        );
        ExitCode::from(exit::PASS)
    } else {
        ExitCode::from(exit::AC_FAIL)
    }
}

/// Build a verdict per pair. Unpaired ACs get the canonical no-test verdict.
/// Paired ACs are sent to the resolved backend.
///
/// A [`backend::Error::Transport`] failure (network/subprocess hiccup,
/// including a per-call timeout) produces a conservative `behavior_match:
/// partial` verdict so one flaky call doesn't block the whole run, with the
/// error surfaced in `reasoning`. A [`backend::Error::BadResponse`] — the
/// backend replied with something that is not the strict JSON verdict — is a
/// hard failure per AC8: this returns `Err` immediately, naming the AC, and
/// the caller writes no receipt at all.
fn build_verdicts(
    pairs: &[Pair],
    crate_root: &Path,
    backend: &dyn Backend,
    crate_name: &str,
    cache_dir: &Path,
) -> Result<Vec<Verdict>, String> {
    let mut out = Vec::with_capacity(pairs.len());
    for p in pairs {
        let Some(ref test_abs) = p.test_path else {
            let mut v = Verdict::unpaired(&p.ac.id);
            v.level.clone_from(&p.ac.level);
            out.push(v);
            continue;
        };
        let rel = test_abs
            .strip_prefix(crate_root)
            .unwrap_or(test_abs)
            .display()
            .to_string();
        let test_source = match fs::read_to_string(test_abs) {
            Ok(s) => s,
            Err(e) => {
                out.push(Verdict {
                    ac_id: p.ac.id.clone(),
                    test_path: Some(rel),
                    level: p.ac.level.clone(),
                    behavior_match: ac_judge::schema::BehaviorMatch::Partial,
                    assertion_kind: ac_judge::schema::AssertionKind::Mixed,
                    confidence: 0.0,
                    reasoning: format!("cannot read test file: {e}"),
                });
                continue;
            }
        };
        match judge_one_ac(
            backend,
            &p.ac.id,
            &p.ac.text,
            Some(&rel),
            &test_source,
            crate_name,
            p.ac.index,
            cache_dir,
        ) {
            Ok(mut v) => {
                v.level.clone_from(&p.ac.level);
                out.push(v);
            }
            Err(backend::Error::BadResponse(m)) => {
                return Err(format!("{}: bad response: {m}", p.ac.id));
            }
            Err(e) => out.push(Verdict {
                ac_id: p.ac.id.clone(),
                test_path: Some(rel),
                level: p.ac.level.clone(),
                behavior_match: ac_judge::schema::BehaviorMatch::Partial,
                assertion_kind: ac_judge::schema::AssertionKind::Mixed,
                confidence: 0.0,
                reasoning: format!("judge call failed: {e}"),
            }),
        }
    }
    Ok(out)
}

fn write_receipt(crate_root: &Path, receipt: &Receipt) -> std::io::Result<()> {
    let dir = crate_root.join("target").join("autobuilder");
    fs::create_dir_all(&dir)?;
    let path = dir.join("ac-semantic-judge.json");
    let json = serde_json::to_string_pretty(receipt)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

// `fpr`/`fnr` (false-positive/-negative rate) are the standard abbreviations
// for these metrics; renaming one to dodge the similar-names lint would
// obscure the pairing.
#[allow(clippy::too_many_lines, clippy::similar_names)]
fn calibrate(golden_set: &Path, backend_flag: &str, model: Option<&str>) -> ExitCode {
    let requested = match parse_backend(backend_flag) {
        Ok(r) => r,
        Err(code) => return code,
    };

    // Load and validate the golden set first (offline — no backend needed
    // yet). Exit 2 on malformed input so the caller can distinguish bad data
    // from a deferred network gate.
    let pairs = match ac_judge::calibrate::load_golden_set(golden_set) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ac-judge: cannot load golden set: {e}");
            return ExitCode::from(2);
        }
    };
    if pairs.is_empty() {
        eprintln!("ac-judge: golden set {} is empty", golden_set.display());
        return ExitCode::from(2);
    }
    let counts = ac_judge::calibrate::LabelCounts::tally(&pairs);
    println!(
        "ac-judge: golden set OK — {} pairs (good={}, bad={}, partial={})",
        counts.total(),
        counts.good,
        counts.bad,
        counts.partial
    );

    // The confusion-matrix gate requires a live backend. When none is
    // available the offline portion (loading + reporting the distribution)
    // has already succeeded; exit 3 to signal "deferred — run with a judge
    // backend available to complete the network gate".
    let backend: Box<dyn Backend> = match backend::resolve(requested, model) {
        Ok(b) => b,
        Err(checks) => {
            eprintln!(
                "ac-judge: no judge backend available; confusion-matrix gate deferred (exit 3); checked:"
            );
            for c in &checks {
                eprintln!("  {c}");
            }
            return ExitCode::from(3);
        }
    };
    eprintln!(
        "ac-judge: backend={} model={}",
        backend.name(),
        backend.model()
    );

    // Run the judge against every golden pair and build the confusion matrix.
    let cache_dir = golden_set.join(".cache");
    let mut confusion = ac_judge::calibrate::Confusion::default();
    for (i, gp) in pairs.iter().enumerate() {
        let ac_id = format!("AC{}", i + 1);
        eprint!("  judging {} ({}/{})... ", gp.id, i + 1, pairs.len());
        let verdict = match judge_one_ac(
            backend.as_ref(),
            &ac_id,
            &gp.ac_text,
            None,
            &gp.test_source,
            "golden",
            u32::try_from(i + 1).unwrap_or(u32::MAX),
            &cache_dir,
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("FAILED: {e}");
                return ExitCode::from(2);
            }
        };
        let judged_failing = verdict.fails_gate();
        confusion.record(gp.label, judged_failing);
        eprintln!(
            "{:?}/{:?} conf={:.2} {}",
            verdict.behavior_match,
            verdict.assertion_kind,
            verdict.confidence,
            if judged_failing { "FAIL" } else { "pass" }
        );
    }

    let fpr = confusion.false_positive_rate();
    let fnr = confusion.false_negative_rate();
    println!(
        "ac-judge calibrate: FPR={fpr:.3} FNR={fnr:.3} tp={} fp={} tn={} fn={}",
        confusion.true_positive,
        confusion.false_positive,
        confusion.true_negative,
        confusion.false_negative,
    );
    if confusion.passes_gate() {
        println!("ac-judge calibrate: PASS (FPR<0.10, FNR<0.20)");
        ExitCode::from(exit::PASS)
    } else {
        eprintln!("ac-judge calibrate: FAIL — FPR={fpr:.3} FNR={fnr:.3} (gates: <0.10, <0.20)");
        ExitCode::from(exit::AC_FAIL)
    }
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
            let level_suffix = verdict
                .level
                .as_deref()
                .map_or_else(String::new, |l| format!(" level={l}"));
            println!(
                "ac-judge show: backend={} model={}{level_suffix}",
                receipt.backend, receipt.model
            );
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
