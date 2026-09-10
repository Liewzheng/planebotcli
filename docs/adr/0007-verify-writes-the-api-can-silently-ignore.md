---
status: accepted
date: 2026-08-18
decision-makers: pbotcli maintainers
---

# ADR-0007: Verify writes the API can silently ignore

> **Historical reference.** This ADR records a decision made for the Python implementation
> that has since been removed from this repository. It is kept for history; the current CLI is
> the Rust line under `crates/`.

## Context and Problem Statement

Not every Plane API write that answers `HTTP 200` actually changed anything. Intake triage is
the concrete case: the intake `PATCH` endpoint builds and applies its serializer **only** when
the caller's project role is Admin. For every lower role the handler falls through and returns
`HTTP 200` with the *unchanged* record in the body — no `403`, no error field, a well-formed
success response.

The CLI's error contract assumes the opposite: a call that does not raise is a call that
worked (`handle_api_error` only ever sees SDK exceptions — see
[ADR-0001](0001-async-wrapper-over-sync-sdk.md)). Under that assumption
`pbot intake accept <issue-uuid> -p Frontend` printed **"Intake Item Accepted"** and exited
`0` for a Member while the item stayed `pending`. The failure was invisible in both output
streams, and a script consuming `--json` could not tell the two outcomes apart.

Retry does not help — the call is not transient, it is authoritatively ignored.

## Considered Options

- **A. Verify the response payload** — compare the field the command intended to change against
  the value the API echoed back, and raise when they differ.
- **B. Pre-flight the caller's role** — fetch the current user's project membership before the
  write and refuse early when the role is below Admin.
- **C. Trust the status code** — keep reporting success on `HTTP 200` and mention the role
  requirement in `--help` only.

## Decision Outcome

Chosen option: **A**, because it asserts the *outcome* (the record now holds the requested
value) instead of a *precondition* (the caller looked entitled). That also catches a rejected
transition, an upstream change to Plane's role matrix, or any future endpoint that answers
`200` without applying the write.

Option B costs an extra round trip per mutation and re-implements Plane's role matrix in the
client, where it would drift silently. Option C leaves a silent-failure bug in place.

The contract:

- After a write whose success is not self-evident, read the **raw** response payload
  (`model_dump()`) and compare the target field against what was requested.
- On mismatch raise `APIError` (exit code **4**) with a message naming the observed state *and*
  the likely cause — e.g. `the intake status was not changed (still 'pending'). Triaging intake
  items requires the project Admin role.`
- Verify **before** display enrichment. Enrichment replaces raw values with human labels
  (`-2` → `pending`), so a check written against enriched data compares a label to an integer
  and can never match.
- Enrichment helpers therefore must not mutate their input: `_enrich_intake` copies
  (`data = {**data}`) so the raw payload survives for the check.

This applies only where the API is known to accept-and-ignore. It is not a blanket read-back
after every mutation; ordinary writes still trust a response that did not raise.

## Confirmation

`commands/intake.py::_set_status` runs the comparison for both `intake accept` and
`intake decline`; `tests/test_commands/test_intake.py` covers the mismatch path — a `200`
echoing the old status must raise, not print success.

## Consequences

**Positive:**
- A non-Admin triage attempt fails loudly with exit `4` instead of a false success.
- The guard is outcome-based, so it survives changes to which roles may triage.
- Exit codes mean what they say again: `0` from a mutation means the mutation happened.

**Negative:**
- The exit code is `4` (API error), not a permission-specific one — the CLI infers the cause
  from the payload and cannot prove it was the role.
- The check couples the command to the response payload's shape; a field rename upstream would
  turn a working write into a reported failure.

**Neutral:**
- Commands that need this also need the raw payload, which constrains where enrichment may run.
