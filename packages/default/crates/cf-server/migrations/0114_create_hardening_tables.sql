-- Systemd hardening analysis schema for NixOS configuration scanning
-- ============================================================================
-- HARDENING SCAN TABLES
-- ============================================================================

-- Hardening scan metadata (similar to cve_scans)
CREATE TABLE hardening_scans (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Link to derivation (NixOS system configuration)
    derivation_id integer REFERENCES derivations(id) ON DELETE CASCADE NOT NULL,
    -- Scan timing
    scheduled_at timestamptz DEFAULT NOW(),
    started_at timestamptz,
    completed_at timestamptz,
    -- Status tracking
    status varchar(20) NOT NULL DEFAULT 'pending',
    attempts integer NOT NULL DEFAULT 0,
    -- Aggregate results
    total_services integer NOT NULL DEFAULT 0,
    -- Score distribution counts
    well_hardened_count integer NOT NULL DEFAULT 0,  -- 80-100
    moderately_hardened_count integer NOT NULL DEFAULT 0,  -- 60-79
    poorly_hardened_count integer NOT NULL DEFAULT 0,  -- 40-59
    vulnerable_count integer NOT NULL DEFAULT 0,  -- 0-39
    -- Overall system score (average of all services)
    overall_score integer,  -- 0-100
    -- Scan performance
    scan_duration_ms integer,
    -- Additional metadata (error details, etc.)
    scan_metadata jsonb,
    created_at timestamptz NOT NULL DEFAULT NOW(),
    
    CONSTRAINT hardening_scan_status_check CHECK (
        status IN ('pending', 'in_progress', 'completed', 'failed')
    ),
    CONSTRAINT hardening_scan_score_range CHECK (
        overall_score IS NULL OR (overall_score >= 0 AND overall_score <= 100)
    )
);

-- Per-service hardening results
CREATE TABLE service_hardening_results (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    scan_id uuid REFERENCES hardening_scans(id) ON DELETE CASCADE NOT NULL,
    -- Service identification
    service_name varchar(255) NOT NULL,
    service_type varchar(50) DEFAULT 'simple',  -- simple, oneshot, forking, etc.
    -- Hardening score (0-100)
    hardening_score integer NOT NULL,
    -- Risk level derived from score
    risk_level varchar(20) NOT NULL,
    -- Individual directive scores (stored as JSON for flexibility)
    -- Contains: { "PrivateTmp": { "enabled": true, "value": true, "points": 5 }, ... }
    directives_detail jsonb NOT NULL DEFAULT '{}',
    -- Summary counts
    enabled_directives_count integer NOT NULL DEFAULT 0,
    disabled_directives_count integer NOT NULL DEFAULT 0,
    missing_directives_count integer NOT NULL DEFAULT 0,
    -- Timestamps
    created_at timestamptz NOT NULL DEFAULT NOW(),
    
    CONSTRAINT service_score_range CHECK (
        hardening_score >= 0 AND hardening_score <= 100
    ),
    CONSTRAINT service_risk_level_check CHECK (
        risk_level IN ('well_hardened', 'moderately_hardened', 'poorly_hardened', 'vulnerable')
    ),
    -- Unique service per scan
    CONSTRAINT uq_scan_service UNIQUE (scan_id, service_name)
);

-- Justifications for service hardening findings (operator risk acceptance)
CREATE TABLE hardening_justifications (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Link to system (not scan, so justifications persist across scans)
    system_id uuid NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    -- Service identification
    service_name varchar(255) NOT NULL,
    -- Optional: specific directive being justified (null = entire service)
    directive_name varchar(100),
    -- Justification details
    category varchar(50),  -- 'required_capability', 'legacy_service', 'external_hardening', etc.
    reason text NOT NULL,
    -- Audit trail
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT NOW(),
    updated_at timestamptz NOT NULL DEFAULT NOW(),
    -- Expiration (optional)
    expires_at timestamptz,
    
    -- Unique justification per system/service/directive combination
    CONSTRAINT uq_hardening_justification UNIQUE (system_id, service_name, directive_name)
);

-- ============================================================================
-- INDEXES FOR PERFORMANCE
-- ============================================================================

-- Hardening scans indexes
CREATE INDEX idx_hardening_scans_derivation_id ON hardening_scans(derivation_id);
CREATE INDEX idx_hardening_scans_status ON hardening_scans(status);
CREATE INDEX idx_hardening_scans_completed_at ON hardening_scans(completed_at);
CREATE INDEX idx_hardening_scans_derivation_status ON hardening_scans(derivation_id, status);

-- Service results indexes
CREATE INDEX idx_service_hardening_results_scan_id ON service_hardening_results(scan_id);
CREATE INDEX idx_service_hardening_results_service_name ON service_hardening_results(service_name);
CREATE INDEX idx_service_hardening_results_risk_level ON service_hardening_results(risk_level);
CREATE INDEX idx_service_hardening_results_score ON service_hardening_results(hardening_score);

-- Justifications indexes
CREATE INDEX idx_hardening_justifications_system_id ON hardening_justifications(system_id);
CREATE INDEX idx_hardening_justifications_service ON hardening_justifications(service_name);
CREATE INDEX idx_hardening_justifications_expires ON hardening_justifications(expires_at)
    WHERE expires_at IS NOT NULL;

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- Calculate risk level from score
CREATE OR REPLACE FUNCTION hardening_risk_level(score integer)
RETURNS varchar(20)
AS $$
BEGIN
    IF score >= 80 THEN
        RETURN 'well_hardened';
    ELSIF score >= 60 THEN
        RETURN 'moderately_hardened';
    ELSIF score >= 40 THEN
        RETURN 'poorly_hardened';
    ELSE
        RETURN 'vulnerable';
    END IF;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- ============================================================================
-- VIEWS
-- ============================================================================

-- Fleet-wide hardening summary (for dashboard)
CREATE OR REPLACE VIEW view_hardening_fleet_summary AS
SELECT
    COUNT(DISTINCT hs.derivation_id) AS total_systems_scanned,
    AVG(hs.overall_score) AS avg_fleet_score,
    SUM(hs.well_hardened_count) AS total_well_hardened_services,
    SUM(hs.moderately_hardened_count) AS total_moderately_hardened_services,
    SUM(hs.poorly_hardened_count) AS total_poorly_hardened_services,
    SUM(hs.vulnerable_count) AS total_vulnerable_services,
    SUM(hs.total_services) AS total_services_scanned,
    MAX(hs.completed_at) AS last_scan_completed
FROM hardening_scans hs
WHERE hs.status = 'completed'
  AND hs.completed_at = (
      SELECT MAX(hs2.completed_at)
      FROM hardening_scans hs2
      WHERE hs2.derivation_id = hs.derivation_id
        AND hs2.status = 'completed'
  );

-- Top vulnerable services across fleet (for dashboard drill-down)
CREATE OR REPLACE VIEW view_hardening_top_vulnerable_services AS
SELECT
    shr.service_name,
    COUNT(DISTINCT hs.derivation_id) AS affected_systems_count,
    AVG(shr.hardening_score) AS avg_score,
    MIN(shr.hardening_score) AS min_score,
    MAX(shr.hardening_score) AS max_score
FROM service_hardening_results shr
JOIN hardening_scans hs ON shr.scan_id = hs.id
WHERE hs.status = 'completed'
  AND hs.completed_at = (
      SELECT MAX(hs2.completed_at)
      FROM hardening_scans hs2
      WHERE hs2.derivation_id = hs.derivation_id
        AND hs2.status = 'completed'
  )
  AND shr.risk_level IN ('vulnerable', 'poorly_hardened')
GROUP BY shr.service_name
ORDER BY affected_systems_count DESC, avg_score ASC
LIMIT 20;

-- System hardening posture (for system detail view)
CREATE OR REPLACE VIEW view_system_hardening_posture AS
SELECT
    d.id AS derivation_id,
    d.derivation_name AS config_name,
    s.id AS system_id,
    s.hostname,
    hs.id AS latest_scan_id,
    hs.overall_score,
    hardening_risk_level(hs.overall_score) AS risk_level,
    hs.total_services,
    hs.well_hardened_count,
    hs.moderately_hardened_count,
    hs.poorly_hardened_count,
    hs.vulnerable_count,
    hs.completed_at AS last_scan_at,
    hs.scan_duration_ms
FROM derivations d
LEFT JOIN systems s ON (
    s.flake_id = (SELECT flake_id FROM commits WHERE id = d.commit_id)
    AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = d.derivation_name
    AND s.is_active = TRUE
)
LEFT JOIN LATERAL (
    SELECT *
    FROM hardening_scans hs2
    WHERE hs2.derivation_id = d.id
      AND hs2.status = 'completed'
    ORDER BY hs2.completed_at DESC
    LIMIT 1
) hs ON true
WHERE d.derivation_type = 'nixos';
