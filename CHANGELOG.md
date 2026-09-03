# Changelog

## v0.2.1 — 2026-09-03

`ac-judge` today only recognized acceptance criteria written as `**AC<N>**: …`
bullets and only paired them with tests named `ac<N>_*.rs` / `acceptance_ac<N>.rs`
/ `fn ac<N>_`. Every PRD in this workspace follows the `/build` contract instead —
a numbered list under `## Acceptance criteria` with lines like
`1. P0 — Given …, When …, Then …` — and rustbuild's own scaffold convention writes
tests as `tests/ac01_*.rs` (zero-padded). The judge therefore reported "no ACs
found" (exit 2) on contract-form PRDs and would have paired nothing even if it
parsed them. This patch teaches `parse_acs` the contract form (section-scoped,
level prefix captured), keeps the old form working, and makes the three pairing
heuristics accept zero-padded indices. No change to the prompt, backends, cache,
or pass/fail rule. Verdicts and the receipt schema gain an optional `level`
field (`P0`/`P1`/`P2`); `ac-judge show` prints it when present.

## v0.2.0 — 2026-09-03

`ac-judge run` gains `--backend auto|codex|api|claude-cli` (default `auto`).
`auto` prefers **Codex** (`codex exec`, authenticated by `codex login` against a
ChatGPT account — a genuinely different model family from the Claude implementer),
then the existing Anthropic Messages API path when `$ANTHROPIC_API_KEY` is set,
then the Claude Code CLI in headless mode (`claude -p`, authenticated by the
Claude login already on every fleet node). Exit 6 now means "no judge backend
available" and says which three things were checked. The receipt records which
backend and model judged each run. Nothing about the prompt, the verdict schema,
the cache, or the Stage 4 gate contract changes.
