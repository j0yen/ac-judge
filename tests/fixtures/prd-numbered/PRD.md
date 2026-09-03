# PRD — fixture: pure /build-contract numbered form

- Status: fixture
- build_target: rust

## TL;DR

A minimal PRD written entirely in the `/build` contract's numbered form, used
to regression-test `parse_acs` pass 2 (the numbered-form recognizer) end to
end against a realistic document: numbered lines appear under `## Requirements`
and `## Success metrics` too, and must not be picked up as ACs.

## Requirements

**P0**

1. The server must accept connections.
2. The server must log every request.

## Success metrics

| metric | target |
|---|---|
| uptime | 99.9% |

1. latency under 50ms
2. zero data loss

## Acceptance criteria

1. P0 — Given a fresh server, When a client connects, Then the handshake
   completes within 100ms.
2. P0 — Given an authenticated client, When it calls `list`, Then it
   receives every item it owns and no others.
3. P1 — Given a malformed request, When it is received, Then the server
   responds with a structured error and stays up.

## Open questions

| question | owner |
|---|---|
| none | n/a |
