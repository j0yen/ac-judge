# ac-judge

A Rust CLI that asks, for each acceptance criterion in a PRD, whether the test claiming to verify it actually exercises the behavior the AC describes.

Mutation testing and semantic judging answer different questions. Mutation testing asks whether a test would catch a broken implementation. `ac-judge` asks the question before that one: does the test check the right thing at all? A test can survive every mutant and still be tautological — calling the function and asserting it returned what it returned. That test proves nothing about the AC, and nothing downstream will tell you so. `ac-judge` reads the AC's English and the test's source together and decides whether the second is really evidence for the first.

It pairs each AC with its test, sends both to Claude (`claude-sonnet-4-6` by default), and asks two strict questions:

1. Does the test exercise the behavior the AC describes? (`behavior_match`)
2. Is the test asserting the AC's stated invariant, or merely re-running the implementation and confirming its own return? (`assertion_kind`)

The judge model is deliberately a different family from the autobuilder's default (Opus). The model that wrote a test should not be the one deciding whether that test verifies its AC — the independence is the whole point.

## Recent

- `--backend auto|codex|api|claude-cli` (default `auto`): the judge now resolves codex (`codex exec`, preferred — a different model family from the Claude implementer) first, then the Anthropic API (`$ANTHROPIC_API_KEY`), then `claude login`'s headless CLI (`claude -p`) as the last resort. An explicit `--backend` never substitutes another one; the receipt records which backend and model actually judged.

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

# Check the judge against a hand-curated golden set; report the confusion matrix.
ac-judge calibrate --golden-set golden/

# Pretty-print one verdict from the most recent run.
ac-judge show --slug AC1 --crate-root <path/to/crate>
```

`run` exits:

- `0` — every AC passed the judge.
- `4` — an AC failed the gate: `behavior_match: no`, or `assertion_kind: restates-impl` with `confidence >= 0.7`.
- `6` — no judge backend available (`codex login`, `$ANTHROPIC_API_KEY`, and `claude login` were all checked and none worked, or the single explicitly `--backend`-requested one didn't). The check runs before any network call, so an unavailable backend never costs a request.

## How pairing works

For a PRD with numbered ACs (`**AC1**: ...`), each AC is matched to a test by these rules, first match wins:

1. `tests/ac<N>_*.rs` — the current autobuilder convention.
2. `tests/acceptance_ac<N>.rs` — the older convention (agorabus, episodic-observer).
3. A `#[test]` whose name starts with `ac<N>_`, in any test file.
4. Otherwise `unpaired`, recorded as `behavior_match: no, reason: "no paired test found"`.

## The verdict

Each verdict is strict JSON against `schemas/ac-semantic-judge.schema.json`:

```json
{
  "ac_id": "AC1",
  "test_path": "tests/ac1_basic.rs",
  "behavior_match": "yes | no | partial",
  "assertion_kind": "asserts-invariant | restates-impl | mixed",
  "confidence": 0.0,
  "reasoning": "<1-2 sentences>"
}
```

`asserts-invariant` means the test checks a property the AC's English actually states — "output ends with cut bytes" becomes an assertion that the last bytes are `0x1D 0x56 0x42 0x00`. `restates-impl` means the test calls the function and asserts it returned what it returned. The first is evidence; the second is a tautology wearing a test's clothes.

Verdicts are collected into a receipt at `target/autobuilder/ac-semantic-judge.json` — the ninth receipt in the autobuilder's risk gate. Stage 4 blocks the build on any failing AC.

## Cost and caching

A cached system block plus few-shot examples keeps a judgment at roughly $0.005 per AC at Sonnet rates — about $0.05 for a ten-AC crate. Re-running on an unchanged AC and test (same SHA-256 of `ac_text + test_source + model + prompt_version`) returns the cached verdict from `target/autobuilder/ac-judge-cache/` without a second call.

## Privacy

Only the AC text and its paired test source go to the API. The rest of the PRD body, the journal, and the environment do not.

## Where it fits

`ac-judge` is the semantic gate inside the autobuilder pipeline — the ninth proof receipt alongside the mutation, coverage, and clippy gates. It runs at Stage 4, after the tests are written and before the build is allowed to claim it proved anything.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
