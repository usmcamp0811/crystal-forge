-- Add closure package counts to nixos-type derivations.
-- closure_total: total number of packages in the system closure (.drv requisites)
-- closure_cached: number of those packages already present in the nix store
-- These are populated asynchronously after eval completes.
ALTER TABLE derivations
    ADD COLUMN IF NOT EXISTS closure_total  integer,
    ADD COLUMN IF NOT EXISTS closure_cached integer;
