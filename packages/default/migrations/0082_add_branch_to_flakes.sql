ALTER TABLE flakes
ADD COLUMN branch text NOT NULL DEFAULT 'main';
