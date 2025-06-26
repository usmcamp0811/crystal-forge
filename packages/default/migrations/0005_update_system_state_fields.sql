ALTER TABLE tbl_system_states
    ADD COLUMN IF NOT EXISTS chassis_serial text,
    ADD COLUMN IF NOT EXISTS bios_version text,
    ADD COLUMN IF NOT EXISTS cpu_microcode text,
    ADD COLUMN IF NOT EXISTS network_interfaces jsonb,
    ADD COLUMN IF NOT EXISTS primary_mac_address text,
    ADD COLUMN IF NOT EXISTS primary_ip_address text,
    ADD COLUMN IF NOT EXISTS gateway_ip text,
    ADD COLUMN IF NOT EXISTS selinux_status text,
    ADD COLUMN IF NOT EXISTS tpm_present boolean,
    ADD COLUMN IF NOT EXISTS secure_boot_enabled boolean,
    ADD COLUMN IF NOT EXISTS fips_mode boolean,
    ADD COLUMN IF NOT EXISTS agent_version text,
    ADD COLUMN IF NOT EXISTS agent_build_hash text,
    ADD COLUMN IF NOT EXISTS nixos_version text,
    ADD COLUMN IF NOT EXISTS systemd_version text;

