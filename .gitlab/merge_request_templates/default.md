# Summary

<!--
In 3–5 sentences, explain in plain language what this MR does and why.
Avoid implementation detail here.
Assume a reviewer unfamiliar with the code is reading this.
-->

Closes: #

---

# Problem Statement

<!--
What problem does this solve?
What was broken, missing, unclear, or inefficient?
Reference related tasks or issues if applicable.
-->

---

# Implementation Overview

<!--
Provide a high-level explanation of the approach taken.
Describe architectural decisions and reasoning.
Mention tradeoffs or rejected alternatives if relevant.
Do not repeat the diff.
-->

---

# UI Changes (if applicable)

<!--
If this affects the UI, include screenshots or before/after comparisons below.
Remove this section if not applicable.
-->

## Before

<!-- Screenshot or description -->

## After

<!--
Screenshot or description
-->

---

# Example Usage (if applicable)

<!--
Provide example commands, API calls, UI flows, or code snippets
demonstrating how the new behavior works.
Remove this section if not applicable.
-->

---

# Verification

## Automated

- [ ] nix flake check passes
- [ ] cargo test passes
- [ ] cargo clippy -- -D warnings passes
- [ ] cargo fmt -- --check passes

## Manual

<!--
Describe what you manually tested and how.
Include edge cases, error cases, and regression checks.
-->

- [ ] Feature verified locally
- [ ] Edge cases tested
- [ ] No regressions observed

---

# Architectural Impact

<!--
Confirm how this change respects architectural boundaries.
If it introduces structural changes, explain them here.
-->

- [ ] No business logic added to UI
- [ ] Layer boundaries preserved
- [ ] No hidden global state introduced
- [ ] Follows existing repository patterns

---

# Data and Schema Changes (if applicable)

<!--
If this change modifies database schema, persistence layer,
or data contracts, explain the impact here.
Remove this section if not applicable.
-->

- [ ] No schema changes
- [ ] Migration added
- [ ] Backward compatible
- [ ] Requires data backfill

Migration file:

<!-- path/to/migration -->

Rollback strategy:

<!--
Explain how to revert safely
-->

---

# Breaking Changes

<!--
If this introduces breaking changes, describe:
- What breaks
- Who is affected
- Migration path
Remove this section if not applicable.
-->

- [ ] No breaking changes
- [ ] Breaking changes documented above

---

# Security Considerations

<!--
Explain any security implications.
Mention dependency additions, validation changes, or privilege adjustments.
-->

- [ ] No new external dependencies
- [ ] Dependencies pinned
- [ ] No secrets introduced
- [ ] Input validation preserved

---

# Performance Considerations

<!--
Explain performance impact.
If measurable, describe benchmarks or reasoning.
Remove this section if not applicable.
-->

- [ ] No measurable performance impact
- [ ] Performance improved
- [ ] Potential regression explained above

---

# Documentation

<!--
Describe documentation updates if needed.
Remove this section if not applicable.
-->

- [ ] README updated
- [ ] Inline docs added
- [ ] API docs updated
- [ ] No documentation needed

---

# Follow-Up Tasks

<!--
List any intentionally deferred improvements discovered during implementation.
These should already exist as Backlog tasks.
-->

---

# Reviewer Guidance

<!--
Help the reviewer focus.
Suggest review order or areas that require special attention.
-->
