# Runbook

* [Web UI check runbook](web-ui-check.md) - Runbook for the web-ui Nix check and the host-side web-ui-test loop, covering layout, phases and gates, visual baselines, design-parity evidence, adding coverage, debugging, CI integration, and known issues.

# Testing Guide

* [Crystal Forge flake checks catalog](flake-checks.md) - Catalogs the current Nix flake-check matrix, standalone Web UI job, documentation and schema checks, and the cf-test-suite scenario runner; states what each verifies, how to run it, and where its README lives.
* [Crystal Forge testing plan](test-plan.md) - Defines the test levels, pytest markers, scenario system, coverage targets, and maintenance goals for Crystal Forge, with open questions and next steps; open it to learn the intended test strategy.
* [Design golden fixtures contract](design-golden-fixtures.md) - Pointer and status record for the design handoff's golden fixtures README, which defines the deterministic crystal-forge.fixtures.json snapshot that the design example, the fixture seeder, and the screenshot checks share.
* [Evaluation snapshot verification expectations](evaluation-snapshot-verification-expectations.md) - Lists the targeted evidence required for any change to evaluation and flake snapshot architecture, covering PRIMARY isolation, redaction, bounds, identity, non-disclosure, deployment queue behavior, API behavior, and compatibility.
* [Fixture seeding developer guide](fixture-seeding.md) - Developer guide to fixture mode, where the server seeds its local development database from the golden fixture JSON for run-ui-dev; distinguishes this stack from the Playwright-mocked ui-screenshots check and documents the seeder's current contents.
* [Offline flake input prefetching for NixOS VM tests](offline-flake-prefetch.md) - Explains how NixOS VM tests prefetch every flake.lock input into the Nix store and redirect the flake registry to local paths so the tested flake evaluates without network access.
