CREATE TABLE user_preferences (
    user_id UUID PRIMARY KEY
        REFERENCES users(id)
        ON DELETE CASCADE,

    theme TEXT NOT NULL DEFAULT 'dark'
        CHECK (theme IN ('dark', 'light')),

    density TEXT NOT NULL DEFAULT 'comfortable'
        CHECK (density IN ('comfortable', 'compact')),

    sidebar_collapsed BOOLEAN NOT NULL DEFAULT FALSE,

    default_systems_view TEXT NOT NULL DEFAULT 'cards'
        CHECK (default_systems_view IN ('cards', 'table')),

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
