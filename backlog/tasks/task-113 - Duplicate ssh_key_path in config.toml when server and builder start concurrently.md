# Duplicate `ssh_key_path` in config.toml when server and builder start concurrently

---

# Problem
Duplicate `ssh_key_path` configuration in config.toml when server and builder start concurrently, causing configuration conflicts.

---

# Desired Outcome
Server and builder should start without duplicate `ssh_key_path` configuration conflicts in config.toml.

---

# Notes
- Labels: type::bug
- Created: about 12 days ago
- Component affected: configuration management
- Environment: NixOS module deployment

---

# Scope Hint (Optional)
Fix the configuration initialization to prevent duplicate `ssh_key_path` entries when server and builder components start simultaneously.