-- Persist per-system CVE justification notes for operator risk acceptance workflows.

CREATE TABLE IF NOT EXISTS public.system_cve_justifications (
    system_id uuid NOT NULL REFERENCES public.systems(id) ON DELETE CASCADE,
    cve_id text NOT NULL REFERENCES public.cves(id) ON DELETE CASCADE,
    category text,
    reason text NOT NULL,
    updated_by uuid REFERENCES public.users(id) ON DELETE SET NULL,
    updated_at timestamptz NOT NULL DEFAULT NOW(),
    created_at timestamptz NOT NULL DEFAULT NOW(),
    PRIMARY KEY (system_id, cve_id)
);

CREATE INDEX IF NOT EXISTS idx_system_cve_justifications_system_id
    ON public.system_cve_justifications(system_id);
