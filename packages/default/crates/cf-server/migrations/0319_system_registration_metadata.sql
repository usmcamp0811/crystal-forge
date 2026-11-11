-- Operator metadata belongs to the registration, independently of runtime mode.
-- Configuration synchronization omits these columns and preserves their values.
ALTER TABLE systems
    ADD COLUMN tags text[] NOT NULL DEFAULT '{}',
    ADD COLUMN description text;
