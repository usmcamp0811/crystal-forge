Do this to make the SQL tables:

```sql
CREATE TABLE system_state (
id SERIAL PRIMARY KEY,
hostname TEXT NOT NULL,
system_derivation_id TEXT NOT NULL,
inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

GRANT SELECT, INSERT, UPDATE, DELETE ON system_state TO crystal_forge;
GRANT USAGE, SELECT ON SEQUENCE system_state_id_seq TO crystal_forge;
```
