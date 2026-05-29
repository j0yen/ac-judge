# ac-judge

A small Rust CLI that judges whether each acceptance-criterion (AC) test
actually *exercises the behavior its AC describes* — a semantic check that
complements mutation testing.

Mutation testing asks: "would the test catch a broken impl?"
`ac-judge` asks the orthogonal question: "does the test even check the
**right thing**?"

It pairs each AC's English text with the test file that claims to verify it,
sends both to Claude (Sonnet 4.6 by default, with a prompt-cached system
block), and asks two strict questions:

1. Does the test exercise the behavior the AC describes? (`behavior_match`)
2. Is the test asserting the AC's stated invariant, or merely re-running the
   impl and confirming its return? (`assertion_kind`)

Verdicts land in a 9th autobuilder receipt at
`target/autobuilder/ac-semantic-judge.json`. The autobuilder Stage 4 gate
blocks if any AC has `behavior_match: no` **or**
`assertion_kind: restates-impl` with `confidence >= 0.7`.

The judge model is **intentionally a different family** from the autobuilder
pipeline default (Opus): the same model that wrote a test should not also
judge whether that test verifies its AC. The independence is load-bearing.

## Install

```sh
cargo install --path .
# or
./install.sh
```

## Usage

```sh
# Judge every AC in a PRD against the crate's tests.
ac-judge run --prd <path/to/PRD.md> --crate-root <path/to/crate> [--model <id>]
#   exit 0  → all ACs pass the judge
#   exit 4  → an AC failed the gate (behavior_match:no OR restates-impl, conf>=0.7)
#   exit 6  → $ANTHROPIC_API_KEY unset (no network attempted)

# Run the judge against a hand-curated golden set; report the confusion matrix.
ac-judge calibrate --golden-set golden/

# Pretty-print one verdict from the most recent run.
ac-judge show --slug AC1 --crate-root <path/to/crate>
```

## Pair detection

For a PRD with numbered ACs (`**AC1**: ...`), each AC is paired to its test by
these heuristics (first match wins):

1. `tests/ac<N>_*.rs` — today's autobuilder convention.
2. `tests/acceptance_ac<N>.rs` — older convention (agorabus, episodic-observer).
3. A `#[test]` function whose name starts with `ac<N>_` in any test file.
4. Falls through to `unpaired` → recorded with
   `behavior_match: no, reason: "no paired test found"`.

## Verdict schema

Each verdict is strict JSON conforming to
`schemas/ac-semantic-judge.schema.json`:

```json
{
  "ac_id": "AC1",
  "test_path": "tests/ac1_basic.rs",
  "behavior_match": "yes" | "no" | "partial",
  "assertion_kind": "asserts-invariant" | "restates-impl" | "mixed",
  "confidence": 0.0,
  "reasoning": "<1-2 sentences>"
}
```

- `asserts-invariant` — the test asserts a property the AC's English
  describes (e.g. "output ends with cut bytes" → asserts the last bytes are
  `0x1D 0x56 0x42 0x00`).
- `restates-impl` — the test calls the function and asserts the function
  returned what the function returned (tautological).

## Cost

Cached system + few-shot drives cost to ~$0.005/AC at Sonnet rates. Per-crate
cost (10 ACs avg): ~$0.05. Re-running on an identical AC + test (same SHA256
of `ac_text + test_source + model + prompt_version`) returns a cached verdict
from `target/autobuilder/ac-judge-cache/` — no second API call.

## Privacy

Only the AC text and the paired test source are sent to the API — never the
rest of the PRD body, the journal, or the environment.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE) at your option.
