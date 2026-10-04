# Workflow

* [Commit -> Eval -> Build -> Cache -> Deploy Flow](commit-eval-build-cache-deploy-flow.md) - Shows the flow chart of how a commit moves from discovery through evaluation, build, cache push, and pull-based deployment; open it for a one-page picture of the pipeline.
* [Commit -> Eval -> Build -> Cache -> Deploy Sequence](commit-eval-build-cache-deploy-sequence.md) - Shows the sequence diagram and reading guide for commit ingestion, evaluation, build job leases, cache push, and deployment convergence between server, workers, and agent; open it to see ordering, leases, and idempotency rules.
* [Crystal Forge store path flow](store-path-flow.md) - Shows how Crystal Forge evaluates a commit per nixosConfiguration, records expected store paths, builds and caches in parallel, and classifies agents as up-to-date, behind, or unknown; open it to understand per-system store path tracking.
* [Evaluation and Build Queue Pipeline](evaluation-and-build-queue-pipeline.md) - Describes the two-stage commit pipeline (evaluation queue then build queue), its database fields, startup resets, and the single-active-evaluation invariant; open it when changing evaluation or build scheduling.
