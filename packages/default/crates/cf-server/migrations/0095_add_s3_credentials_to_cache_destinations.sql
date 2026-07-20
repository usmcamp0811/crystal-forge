-- Add optional S3 credential fields for cache destinations
ALTER TABLE cache_destinations
ADD COLUMN IF NOT EXISTS s3_access_key_id TEXT,
ADD COLUMN IF NOT EXISTS s3_secret_access_key TEXT,
ADD COLUMN IF NOT EXISTS s3_session_token TEXT,
ADD COLUMN IF NOT EXISTS s3_endpoint_url TEXT;
