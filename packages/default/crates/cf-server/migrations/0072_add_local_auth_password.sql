-- Add password hash field for local authentication
-- This enables local username/password authentication alongside OIDC

ALTER TABLE users
ADD COLUMN password_hash text;

-- Create index for faster lookups during login
CREATE INDEX IF NOT EXISTS idx_users_username ON users (username);

-- Add comment explaining the field
COMMENT ON COLUMN users.password_hash IS 'Argon2id hash for local authentication. NULL for OIDC-only users.';
