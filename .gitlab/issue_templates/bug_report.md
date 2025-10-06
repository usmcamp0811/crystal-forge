## Bug Report

### Description

<!-- Clear description of what's broken -->

### Steps to Reproduce

1.
2.
3.

### Expected Behavior

<!-- What should happen -->

### Actual Behavior

<!-- What actually happens -->

### Environment

- Crystal Forge version: <!-- e.g., v0.1.26 -->
- Component affected: <!-- server / agent / builder -->
- NixOS version: <!-- e.g., 24.05 -->
- Deployment method: <!-- NixOS module / manual / other -->

### Logs

<!-- Paste relevant logs here -->

```
# Server logs: journalctl -u crystal-forge-server
# Agent logs: journalctl -u crystal-forge-agent
# Builder logs: journalctl -u crystal-forge-builder
```

### Database State (if relevant)

<!-- Query results showing the issue -->

```sql
-- Example: SELECT * FROM systems WHERE hostname = 'problematic-system';
```

### Configuration

<!-- Relevant parts of your NixOS config or config.toml -->

```nix
# Sanitize any secrets!
services.crystal-forge = {
  # ...
};
```

### Additional Context

<!-- Anything else that might be relevant -->

### Workaround

<!--
If you found a temporary fix, share it here
-->
