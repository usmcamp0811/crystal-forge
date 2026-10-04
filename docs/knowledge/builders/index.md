# Architecture

* [Builder network flows by execution strategy](builder-network-flows-by-strategy.md) - Shows the network sequence diagrams for the builder job lifecycle and for ServerDerivation, SourceReEvaluateVerified with ServerBundledArchive, and LocalGitWorktree, including the delta derivation protocol security properties.
* [Builder trust boundaries and component definitions](builder-trust-boundaries-and-components.md) - Defines the purpose, trust levels, and component definitions (server, builder, agent) that bound what a Crystal Forge remote builder can reach, hold, and compromise; open it to approve or review builder network and credential exposure.
* [Multi-Builder API architecture, scheduling, and environment assignment](builder-architecture-and-job-scheduling.md) - Describes the multi-builder architecture, environment assignment (wildcard and specific builders), heartbeat and offline detection, query performance, the migration from direct database access, and future enhancements.

# Design Specification

* [Builder failure phases and retry strategy](builder-failure-phases-and-retry.md) - Lists the pre-build failure phases a builder reports (source_fetch through build), which of them retry or fail permanently, and the priority-weighting retry and max-retries rules for build jobs.
* [Remote builder architecture status and follow-up plan (doc-16)](remote-builder-architecture-status-and-follow-up-plan.md) - Pointer to the retained Backlog document doc-16: TASK-375 status of the API-only remote builder, the derivation-transport problem, the server_derivation and source_re_evaluate_verified strategies, the proposed attempt phases, and design principles.
* [Remote builder execution strategies](remote-build-execution-strategies.md) - Explains the remote build execution strategies (source_re_evaluate_verified, server_derivation), the recommended default, source delivery modes, delta derivation materialization, and the forwarded-HTTPS rule for credential-bearing cache push.
* [Verified-source evaluator contract (source_re_evaluate_verified)](verified-source-evaluator-contract.md) - Specifies the verified-source flow where the builder re-evaluates a canonical source archive and compares its .drvPath to the server value, including the evaluator fingerprint, next-job 409 reasons, and rolling-upgrade behavior.
