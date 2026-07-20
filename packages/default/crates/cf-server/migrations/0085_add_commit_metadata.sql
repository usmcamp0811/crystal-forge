-- Add commit message and author fields to commits table
-- These are populated during commit sync to avoid slow git operations during timeline hydration

ALTER TABLE commits
ADD COLUMN message TEXT,
ADD COLUMN author TEXT;

-- Create index on message for potential text search
CREATE INDEX idx_commits_message ON commits USING gin(to_tsvector('english', message));
