# PRD — fixture: agorabus-era bullets + a later /build-contract section

- Status: fixture
- build_target: rust

## TL;DR

Simulates an agorabus-era PRD (`**AC<N>**: ...` bullets, no acceptance
heading required) that has since grown a `/build`-contract-style
`## Acceptance criteria` section too — exercising the union-by-index,
first-occurrence-wins, sort-by-index rule (guardrail: this fixture's AC list
must be identical before and after the two-pass parser lands).

## Problem statement

- **AC1**: the older bullet convention still works anywhere in the document.
- **AC2**: this is the bullet-form text for AC2 — it occurs first in the
  document, so it must win over the numbered form's AC2 text below.
- **AC3**: a third bullet-form AC, unrelated to the numbered section.

## Acceptance criteria

2. P0 — this is the numbered-form text for AC2 — it must lose to the bullet
   form above since the bullet occurs first in the document.
4. P1 — Given a new AC declared only in the numbered form, When parsed,
   Then it appears as AC4 with level P1.
