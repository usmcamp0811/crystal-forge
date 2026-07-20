ALTER TABLE public.system_states
ADD COLUMN IF NOT EXISTS generation integer;
