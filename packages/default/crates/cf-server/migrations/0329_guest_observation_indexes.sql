-- Indexes for the guest observation read and for the foreign keys of an
-- append-only table.
--
-- guest_observations_latest (0315) covers (guest_id, observed_at DESC, id DESC).
-- latest() additionally requires the observation to come from the guest's CURRENT
-- parent. After a guest is re-associated, every observation under the former parent
-- is skipped, and the planner scanned the whole table to do it. Measured with 400,000
-- observations under a former parent and 5 under the current one (plus 300,000
-- others): 277 ms before, 0.24 ms after (EXPLAIN ANALYZE).
--
-- parent_host_id and environment_id are foreign keys without an index, so removing
-- a system or an environment scans the table (the parent and guest keys are
-- covered by the index above through guest_id and parent_host_id respectively).
CREATE INDEX guest_observations_guest_parent_latest
    ON guest_observations (guest_id, parent_host_id, observed_at DESC, id DESC);

CREATE INDEX guest_observations_parent_host_idx ON guest_observations (parent_host_id);

CREATE INDEX guest_observations_environment_idx ON guest_observations (environment_id);
