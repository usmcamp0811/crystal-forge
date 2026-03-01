-- Add database constraints for builder public_key field to prevent DoS
-- TASK-148: Add public key validation for builder registration

-- Constraint: Limit public_key field length (base64-encoded 32 bytes = 44 chars + padding, allow up to 1000 for safety)
ALTER TABLE builders
ADD CONSTRAINT builders_public_key_length_check
CHECK (LENGTH(public_key) > 0 AND LENGTH(public_key) <= 1000);

-- Constraint: Ensure public_key is not NULL (redundant with column definition but explicit)
ALTER TABLE builders
ALTER COLUMN public_key SET NOT NULL;

-- Constraint: Limit builder name length
ALTER TABLE builders
ADD CONSTRAINT builders_name_length_check
CHECK (LENGTH(name) > 0 AND LENGTH(name) <= 255);
