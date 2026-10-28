# Concept

* [Advanced Policy Types](advanced-policy-types.md) - Defines the time_window, require_approvals, canary_rollout, and cve_threshold deployment-time policy types with configuration fields, approval workflow and API endpoints, canary phases and state tracking, and the difference from require_cve_check.
* [Built-in Policy Types](built-in-policy-types.md) - Defines the require_cf_agent, require_packages, and custom_check policy types with their JSON configuration, the legacy single-expression and multi-rule custom_check shapes, and the all and any modes.
* [Composite policy enforcement](composite-policy-enforcement.md) - Defines composite policy schema version 1 with its eight typed rule kinds, authoritative phases, rule result aggregation, evaluation expression rules, assessment scoping, and fail-closed final authorization before target updates.
* [Deployment Policies](deployment-policies.md) - Explains deployment policy architecture, how the deployment manager applies allow, warn, block, and pending decisions to auto_latest systems, policy assignment to environments and systems, and the build-time and deployment-time evaluation flow.
* [Deployment Policy Checks](deployment-policy-checks.md) - Defines the deployment check policy types (require_cf_agent, require_packages, custom_check, require_cve_check, composite), custom_check validation and semantics, require_cve_check applicability and config, and the seeded canonical CVE policies.
* [Deployment policy use cases, best practices, and future enhancements](policy-use-cases-and-best-practices.md) - Gives example policy configurations (approval gate, change window, gradual rollout, zero-tolerance CVE), configuration best practices, and the proposed future enhancements for deployment policies.

# Design Specification

* [Manual deployment queue contract](manual-deployment-queue-contract.md) - Specifies the manual deployment actions (deploy, continue_auto_latest, convert_to_manual), partial-success semantics, and the request_id idempotency and 24-hour legacy replay rules.

# Workflow

* [Agent heartbeat, state, deployment, and history logic](agent-heartbeat-vs-state-persistence.md) - Entry point for how agents report state, how the server chooses between an agent_heartbeats row and a full system_states row, and the equivalence check; open it to understand heartbeat versus full state persistence.
* [Deployment Flow](deployment-flow.md) - Describes how an agent receives and applies a desired target (heartbeat process, result types, configuration) and the automatic and manual deployment paths; open it when working on agent-side or user-triggered deployment.
