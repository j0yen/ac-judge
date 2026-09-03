#!/usr/bin/env bash
# Configurable `codex` CLI stub for ac-judge's hermetic backend tests.
# Understands exactly the two invocations ac-judge makes: `login status`
# and `exec ... --output-last-message <file>`.
#
# Controlled entirely by environment variables so one script serves every
# codex-backend acceptance test:
#   STUB_LOGIN_OK      "1" (default) or "0" — `login status` exit code.
#   STUB_VERDICT        JSON string written as the canned verdict.
#                        Defaults to a passing asserts-invariant verdict.
#   STUB_SLEEP          seconds to sleep before responding to `exec` (for the
#                        per-call timeout test).
#   STUB_NONJSON        "1" — write non-JSON garbage instead of the verdict.
#   STUB_COUNT_FILE      path to append one line to per `exec` invocation
#                        (for the cache-hit-means-zero-calls test).
set -euo pipefail

if [ "${1:-}" = "login" ] && [ "${2:-}" = "status" ]; then
  if [ "${STUB_LOGIN_OK:-1}" = "1" ]; then
    exit 0
  fi
  exit 1
fi

if [ "${1:-}" = "exec" ]; then
  # Consume stdin (the prompt) so the real caller's pipe never blocks.
  cat >/dev/null || true

  if [ -n "${STUB_COUNT_FILE:-}" ]; then
    echo "1" >>"$STUB_COUNT_FILE"
  fi

  if [ -n "${STUB_SLEEP:-}" ]; then
    sleep "$STUB_SLEEP"
  fi

  # Find --output-last-message <path> in argv.
  out=""
  prev=""
  for arg in "$@"; do
    if [ "$prev" = "--output-last-message" ]; then
      out="$arg"
    fi
    prev="$arg"
  done
  if [ -z "$out" ]; then
    echo "codex_stub: missing --output-last-message" >&2
    exit 1
  fi

  if [ "${STUB_NONJSON:-0}" = "1" ]; then
    printf 'not json at all' >"$out"
    exit 0
  fi

  verdict="${STUB_VERDICT:-}"
  if [ -z "$verdict" ]; then
    verdict='{"behavior_match":"yes","assertion_kind":"asserts-invariant","confidence":0.9,"reasoning":"stub verdict"}'
  fi
  printf '%s' "$verdict" >"$out"
  exit 0
fi

echo "codex_stub: unhandled invocation: $*" >&2
exit 1
