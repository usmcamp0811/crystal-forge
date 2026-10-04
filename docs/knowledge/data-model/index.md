* [views/](views/index.md) - Describes view_config_timeline, the Grafana config timeline view that labels each commit with the number of systems currently running it, derived from view_system_deployment_status and view_commit_deployment_timeline.

# Data Model

* [Authoritative system_events timeline](system-events-timeline.md) - Defines the system_events event types, idempotent dedupe keys, deterministic ordering and correlation, and the pending_system_deployments context that attributes Crystal Forge deployments; open it when reading or changing Deployment History data.
* [Builder API database schema](builder-api-database-schema.md) - Lists the builders, builder_environment_assignments, build_jobs, and builder_metrics tables with their SQL definitions, job states, and retry columns used by the multi-builder API.
* [Core entities and relationships](core-entities-and-relationships.md) - Summarizes the core Crystal Forge entities (system, environment, flake, builder, user, deployment, derivation, cache) and their relationships; open it for a quick map of the domain model.
* [Deployment Timeline View - Developer Notes](commit-deployment-timeline-developer-notes.md) - Developer notes for view_commit_deployment_timeline: the failed derivation_path join, the first-seen-after-commit deployment approximation, limitations, alternatives considered, recommendations, usage notes, and monitoring queries.
* [Systems Status View - Technical Notes](systems-status-view-technical-notes.md) - Records why view_systems_status_table showed every system as Unknown State, why derivation_path and nix hash matching failed, and the derivation-name matching solution with its schema dependencies and future considerations.

# Reference

* [Database Relationships and Views Guide](database-relationships-and-views-guide.md) - Generic SQL patterns for views over related tables (latest-record, multiple activity streams, status derivation, hierarchical rollup, time windows) with design principles and pitfalls; open it before writing or changing a status view.
