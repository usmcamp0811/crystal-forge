-- Migration 0107: Seed canonical CVE deployment policies (disabled by default)
--
-- These policies are seeded as disabled. Admins can enable them from the UI
-- or via API when their fleet has CVE scanning active and they wish to gate
-- deployments on CVE posture.
--
-- require_no_critical_cves:
--   Blocks deployment if any critical CVE (CVSS >= 9.0) is found.
--   when_no_scan=block means deployments are also blocked if no scan has run.
--
-- require_high_cve_justification:
--   Blocks deployment if any high CVE (CVSS 7.0-8.9) lacks a whitelist_reason.
--   when_no_scan=skip means deployments proceed if no scan has run yet.

INSERT INTO deployment_policies (name, description, policy_type, config, enabled, created_at, updated_at)
VALUES
  (
    'require_no_critical_cves',
    'Block deployment if the built derivation has any critical CVEs (CVSS >= 9.0). Treats missing scan as a violation.',
    'require_cve_check',
    '{"max_critical": 0, "strict": true, "when_no_scan": "block"}'::jsonb,
    false,
    NOW(),
    NOW()
  ),
  (
    'require_high_cve_justification',
    'Block deployment if any high CVE (CVSS 7.0-8.9) lacks a whitelist justification. Skips check when no scan exists.',
    'require_cve_check',
    '{"require_high_justification": true, "strict": true, "when_no_scan": "skip"}'::jsonb,
    false,
    NOW(),
    NOW()
  )
ON CONFLICT (name) DO NOTHING;
