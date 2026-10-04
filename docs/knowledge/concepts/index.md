# Concept

* [Derivation status lifecycle](derivation-status-lifecycle.md) - Defines the derivation status IDs and names, the combined lifecycle sequence, terminal states, and retry rules; open it when reading or changing derivation status handling.
* [Legacy system status determination logic](legacy-system-status-determination.md) - Historical definition of how view_systems_status_table determined Up to Date, Outdated, Unknown State, and Offline from dry-run success and latest commit; open it only for the old logic, the current contract is the System Deployment Status View.
* [Restart and activation classification](restart-and-activation-classification.md) - Explains how boot_id and generation or store-path transitions classify history rows as Crystal Forge deployment, local rebuild, system restart, agent restart, or state change, and why a startup report can be a local rebuild.
