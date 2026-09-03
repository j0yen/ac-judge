#!/usr/bin/env bash
# Configurable `claude -p` CLI stub for ac-judge's hermetic backend tests.
#
#   STUB_VERDICT       JSON string embedded as the envelope's `.result`.
#                       Defaults to a passing asserts-invariant verdict.
#   STUB_NONJSON        "1" — print plain non-JSON text instead of an envelope.
#   STUB_IS_ERROR       "1" — set `is_error: true` in the envelope.
#   STUB_ARGV_FILE      path to dump argv (one per line) to, for assertions
#                       that specific flags (`--tools ""`, `--max-turns 1`)
#                       were actually passed.
set -euo pipefail

if [ -n "${STUB_ARGV_FILE:-}" ]; then
  printf '%s\n' "$@" >"$STUB_ARGV_FILE"
fi

if [ "${STUB_NONJSON:-0}" = "1" ]; then
  printf 'not a json envelope'
  exit 0
fi

verdict="${STUB_VERDICT:-}"
if [ -z "$verdict" ]; then
  verdict='{"behavior_match":"yes","assertion_kind":"asserts-invariant","confidence":0.9,"reasoning":"stub verdict"}'
fi
is_error="false"
if [ "${STUB_IS_ERROR:-0}" = "1" ]; then
  is_error="true"
fi

# Escape double quotes and backslashes so the verdict JSON embeds cleanly as
# a single string value inside the envelope's .result field.
escaped=$(printf '%s' "$verdict" | sed 's/\\/\\\\/g; s/"/\\"/g')

printf '{"result":"%s","is_error":%s,"num_turns":1}\n' "$escaped" "$is_error"
