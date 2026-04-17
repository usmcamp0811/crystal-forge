# Fix Webhooks to support *.git urls.

---

# Problem
Webhooks do not support *.git URLs for repository configuration.

---

# Desired Outcome
Webhooks should properly handle *.git URLs for repository configuration and commit detection.

---

# Notes
- Labels: feature::flake-tracking
- Created: about 2 months ago
- Component affected: webhook system
- Environment: NixOS module

---

# Scope Hint (Optional)
Update webhook handling logic to properly parse and support *.git URLs for repository configuration.