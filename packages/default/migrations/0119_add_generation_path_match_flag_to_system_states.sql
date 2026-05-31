ALTER TABLE public.system_states
ADD COLUMN IF NOT EXISTS generation_matches_current_store_path boolean;
