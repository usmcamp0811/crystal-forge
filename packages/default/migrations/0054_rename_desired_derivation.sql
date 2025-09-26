-- Rename the column
ALTER TABLE systems RENAME COLUMN desired_derivation TO desired_target;

-- Update any comments or documentation
COMMENT ON COLUMN systems.desired_target IS 'Desired flake target for system deployment (e.g., git+https://example.com/repo?rev=abc123#nixosConfigurations.hostname.config.system.build.toplevel)';

