---
id: TASK-143
title: Implement automatic build job creation for evaluated derivations
status: To Do
assignee: []
created_date: '2026-03-01 02:14'
updated_date: '2026-03-01 16:11'
labels:
  - backend
  - build-system
  - priority
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The build queue is currently empty because build_jobs are not being automatically created when commits are evaluated and derivations are discovered.

## Problem
- Commit evaluation discovers nixosConfigurations and creates derivation records
- But no build_jobs are created from these derivations
- Build queue remains empty even though there are evaluated derivations
- Builders have nothing to build

## Solution
Implement automatic build job creation with smart prioritization:

1. **Create build jobs after successful commit evaluation**
   - When evaluate_with_nix_eval_jobs completes successfully
   - Create one build_job per derivation
   - Set initial status to 'queued'
   - Set builder_id to NULL (unassigned)

2. **Smart prioritization using priority_weight**
   - Higher weight for tracked systems (systems in our systems table)
   - Higher weight for newer commits (commits_behind = 0)
   - Formula: `priority_weight = base_weight * tracked_multiplier * recency_multiplier`
   - Example weights:
     - Tracked system, newest commit: 10.0
     - Tracked system, older commit: 5.0  
     - Untracked config, newest commit: 2.0
     - Untracked config, older commit: 1.0

3. **Prevent duplicate job creation**
   - Check if build_job already exists for derivation_id
   - Use ON CONFLICT DO NOTHING or check first
   - Idempotent job creation

4. **Set environment_id for filtering**
   - Match derivation_target (hostname) to systems table
   - Get environment_id from systems table
   - Use for builder environment assignment filtering

## Implementation locations
- `packages/default/src/server/mod.rs` - After evaluate_with_nix_eval_jobs success
- `packages/default/src/queries/builds.rs` (new file) - create_build_jobs_for_derivations()
- Priority calculation should consider:
  - Is hostname in systems table? (tracked = true)
  - commits_behind value from commit
  - Maybe deployment_policy?

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Build jobs are automatically created when commits are successfully evaluated
- [ ] #2 Tracked systems (in systems table) get higher priority_weight
- [ ] #3 Newer commits (commits_behind=0) get higher priority_weight  
- [ ] #4 No duplicate build_jobs are created for same derivation
- [ ] #5 Build queue API shows queued jobs
- [ ] #6 Builders can pick up jobs and start building
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reality check (2026-03-01): still not implemented on dev. Commit evaluation path in packages/default/src/server/mod.rs evaluates commits but does not create build_jobs automatically.

No INSERT INTO build_jobs path was found in current backend query code for post-evaluation job creation. Task remains needed.
<!-- SECTION:NOTES:END -->
