ALTER TABLE environments
    ADD COLUMN IF NOT EXISTS color_hex varchar(7) DEFAULT '#6B7280';

UPDATE environments
SET color_hex = '#6B7280'
WHERE color_hex IS NULL OR color_hex = '';

ALTER TABLE environments
    ALTER COLUMN color_hex SET NOT NULL;
