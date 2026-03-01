#!/usr/bin/env bash
#
# Seed database with builder test/demo data
#
# Usage:
#   ./scripts/seed_builders.sh
#
# Requirements:
#   - PostgreSQL running (process-compose: db-only up)
#   - DATABASE_URL env var set, or uses default
#

set -euo pipefail

DATABASE_URL="${DATABASE_URL:-postgresql://postgres:postgres@localhost:5432/crystal_forge}"

echo "🌱 Seeding builders data..."

# Generate Ed25519 keypairs for demo builders
generate_keypair() {
    # Generate a random Ed25519 keypair using openssl
    # Returns: private_key_hex public_key_base64
    local private_key=$(openssl rand -hex 32)
    local public_key=$(echo -n "$private_key" | xxd -r -p | openssl dgst -sha512 -binary | head -c 32 | base64)
    echo "$private_key $public_key"
}

# Create builders
echo "Creating builders..."

read PRIV1 PUB1 <<< $(generate_keypair)
read PRIV2 PUB2 <<< $(generate_keypair)
read PRIV3 PUB3 <<< $(generate_keypair)

psql "$DATABASE_URL" <<SQL
-- Clean existing test data
DELETE FROM builder_metrics WHERE builder_id IN (
    SELECT id FROM builders WHERE name LIKE 'demo-%'
);
DELETE FROM builder_environment_assignments WHERE builder_id IN (
    SELECT id FROM builders WHERE name LIKE 'demo-%'
);
DELETE FROM build_jobs WHERE builder_id IN (
    SELECT id FROM builders WHERE name LIKE 'demo-%'
);
DELETE FROM builders WHERE name LIKE 'demo-%';

-- Insert demo builders
INSERT INTO builders (id, name, public_key, status, max_cpu_cores, max_memory_mb, max_concurrent_jobs, last_heartbeat_at, created_at, updated_at)
VALUES
    (
        'aaaaaaaa-1111-4111-8111-111111111111'::uuid,
        'demo-builder-primary',
        '$PUB1',
        'active',
        16,
        32768,
        4,
        NOW() - INTERVAL '30 seconds',
        NOW() - INTERVAL '7 days',
        NOW() - INTERVAL '30 seconds'
    ),
    (
        'bbbbbbbb-2222-4222-8222-222222222222'::uuid,
        'demo-builder-secondary',
        '$PUB2',
        'active',
        8,
        16384,
        2,
        NOW() - INTERVAL '1 minute',
        NOW() - INTERVAL '3 days',
        NOW() - INTERVAL '1 minute'
    ),
    (
        'cccccccc-3333-4333-8333-333333333333'::uuid,
        'demo-builder-offline',
        '$PUB3',
        'offline',
        4,
        8192,
        1,
        NOW() - INTERVAL '2 hours',
        NOW() - INTERVAL '1 day',
        NOW() - INTERVAL '2 hours'
    );

-- Get environment IDs (assumes some environments exist)
DO \$\$
DECLARE
    env_prod_id uuid;
    env_staging_id uuid;
BEGIN
    -- Try to get existing environments
    SELECT id INTO env_prod_id FROM environments WHERE name = 'production' LIMIT 1;
    SELECT id INTO env_staging_id FROM environments WHERE name = 'staging' LIMIT 1;
    
    -- Assign environments to builders (if they exist)
    IF env_prod_id IS NOT NULL THEN
        INSERT INTO builder_environment_assignments (builder_id, environment_id, created_at)
        VALUES
            ('aaaaaaaa-1111-4111-8111-111111111111'::uuid, env_prod_id, NOW()),
            ('bbbbbbbb-2222-4222-8222-222222222222'::uuid, env_prod_id, NOW())
        ON CONFLICT DO NOTHING;
    END IF;
    
    IF env_staging_id IS NOT NULL THEN
        INSERT INTO builder_environment_assignments (builder_id, environment_id, created_at)
        VALUES
            ('bbbbbbbb-2222-4222-8222-222222222222'::uuid, env_staging_id, NOW())
        ON CONFLICT DO NOTHING;
    END IF;
END \$\$;

-- Insert builder metrics (last 24 hours)
INSERT INTO builder_metrics (builder_id, timestamp, cpu_usage_percent, memory_usage_mb, system_cpu_usage_percent, system_memory_total_mb, system_memory_used_mb)
SELECT
    'aaaaaaaa-1111-4111-8111-111111111111'::uuid,
    NOW() - (interval '1 minute' * series),
    20 + (random() * 60)::numeric(5,2),
    8000 + (random() * 8000)::bigint,
    10 + (random() * 30)::numeric(5,2),
    32768,
    12000 + (random() * 8000)::bigint
FROM generate_series(0, 1440, 5) series;  -- Every 5 minutes for 24 hours

INSERT INTO builder_metrics (builder_id, timestamp, cpu_usage_percent, memory_usage_mb, system_cpu_usage_percent, system_memory_total_mb, system_memory_used_mb)
SELECT
    'bbbbbbbb-2222-4222-8222-222222222222'::uuid,
    NOW() - (interval '1 minute' * series),
    15 + (random() * 40)::numeric(5,2),
    4000 + (random() * 4000)::bigint,
    8 + (random() * 20)::numeric(5,2),
    16384,
    6000 + (random() * 4000)::bigint
FROM generate_series(0, 1440, 5) series;

SQL

echo "✅ Builders created:"
echo "   - demo-builder-primary (active, 16 cores, 32GB, 4 jobs)"
echo "   - demo-builder-secondary (active, 8 cores, 16GB, 2 jobs)"
echo "   - demo-builder-offline (offline, 4 cores, 8GB, 1 job)"
echo ""
echo "📊 Metrics seeded for last 24 hours"
echo ""
echo "🔑 Demo private keys (for testing API auth):"
echo "   Primary:   $PRIV1"
echo "   Secondary: $PRIV2"
echo "   Offline:   $PRIV3"
echo ""
echo "✨ Done!"
