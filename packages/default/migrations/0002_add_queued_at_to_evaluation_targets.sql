ALTER TABLE tbl_evaluation_targets
  DROP COLUMN queued,
  ADD COLUMN queued_at timestamptz DEFAULT now();
