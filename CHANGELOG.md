# Changelog

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
