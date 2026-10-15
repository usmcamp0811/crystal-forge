---
type: Decision
title: TASK-470 review decisions
description: Records the task owner's Niks3 verification exceptions and their limits without granting merge approval.
tags:
  - crystal-forge
  - niks3
  - verification
implementation_status: implemented
---

# TASK-470 review decisions

This document records verification exceptions selected by the task owner during
implementation of [MR !331](https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/331).
The decisions permit review with the stated gaps. They do not approve merging,
waive security findings, or establish that a failed or unexecuted test passed.
GitLab approval rules and approving-user records are separate from these decisions.

## Full Web UI baseline exception

Before opening the original MR, the agent asked:

> The Niks3 authoritative browser workflow now passes. The full Web UI suite still
> fails on 34 other workflows and five strict visual baselines; an actual run of
> the original origin/dev baseline reproduced those defects. May I open the
> Niks3 MR after final focused checks, documenting the full-suite failure and
> baseline evidence rather than fixing unrelated UI workflows in this task?

The owner selected **Open MR with evidence**. The option explicitly authorized an
exception to the full-suite-green gate, with unrelated failures left unchanged
and exact outcomes recorded in the MR.

The baseline was commit `4fb7d960909bb7dc75d5f859af43e38f7a3c657e`.
Its full Web UI run reported 104 of 139 workflows passed and five strict TASK-433
visual differences. The original Niks3 revision reported 105 of 139 workflows
passed, with the Niks3 workflow passing and the same remaining defects. These
are historical results, not verification of a rebased or later revision.

## Legacy cache regression exception

The agent then asked:

> The final Niks3 VM and focused authoritative UI checks pass. I also enabled the
> existing Attic/S3 regression phases, but their unchanged fixture expects
> minio.service while the VM supplies Garage; setup fails before either suite
> runs. May I include this baseline fixture blocker as an explicit verification
> gap in the MR, without expanding Niks3 work to repair those legacy test fixtures?

The owner selected **Open MR; document gap**. The option required an explicit
statement that the Attic/S3 suites had not passed or reached their test bodies,
the exact fixture failure, and a follow-up task.

The recorded failure is in the shared Python fixture:
`packages/cf-test-suite/cf_test/tests/conftest.py` waits for `minio.service` and
port 9000, while the Web UI VM uses Garage and port 3900. TASK-475 tracks repair
of this fixture and its regression entry point. An ordinary green integration or
server-regressions job does not establish that these optional cache suites ran.

## Acceptance change

The project Backlog.md record for TASK-470 was amended to require:

> Packaging preserves evaluator Nix selection; server and builder packages plus
> Niks3 integration and focused authoritative Web UI gates pass; full-suite and
> legacy-cache regression outcomes are recorded under the user's explicit review
> exceptions.

The amendment changes the verification gate only. Atomic cache scope, capability
fencing, authoritative cache selection, signature verification, and secret
handling remain requirements. The request-changes review of `56b732b1` is not
overridden by either exception.

## Rebase and migration numbering

After the request-changes review, the owner requested a rebase onto `origin/dev`.
The rebased target included migrations through 0298, including different 0269,
0270, and 0271 migrations. The agent reported the collision and stopped database
verification before changing migration identities.

The owner explicitly instructed:

> renumber the new migrations from this work.. they should come after whats on
> dev now

Only this MR's migration files were renumbered to 0299, 0300, and 0301. Their SQL
contents were retained. Target-branch migrations were not changed. The old
task-preview database was preserved; the rebased preview uses a separate
task-owned database. A database that applied the superseded MR-only numbering
must not be upgraded by rewriting its migration history implicitly.

## Publishing the rebased history

The owner separately selected **Yes, force-with-lease** to update only
`TASK-470-niks3-cache-support`, guarded by the exact previous remote SHA
`56b732b1f85b8495327d604031a38056d78460b4`. The authorized push must refuse an
intervening remote update. This authorization does not permit merging the MR.
