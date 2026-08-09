CREATE TABLE compliance_grouping_schemes (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    groups JSONB NOT NULL,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT compliance_grouping_schemes_name_unique UNIQUE (name)
);
