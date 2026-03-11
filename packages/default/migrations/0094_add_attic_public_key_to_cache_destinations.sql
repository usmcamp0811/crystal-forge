-- Add Attic public key so agents can trust server-managed cache keys
ALTER TABLE cache_destinations
ADD COLUMN IF NOT EXISTS attic_public_key TEXT;
