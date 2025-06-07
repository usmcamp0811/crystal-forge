Do this to make the SQL tables:

```sql
CREATE TABLE system_state (
    id SERIAL PRIMARY KEY,
    hostname TEXT NOT NULL,
    system_derivation_id TEXT NOT NULL,
    context TEXT NOT NULL,

    os TEXT,
    kernel TEXT,
    memory_gb DOUBLE PRECISION,
    uptime_secs BIGINT,
    cpu_brand TEXT,
    cpu_cores INT,
    board_serial TEXT,
    product_uuid TEXT,
    rootfs_uuid TEXT,

    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

GRANT SELECT, INSERT, UPDATE, DELETE ON system_state TO crystal_forge;
GRANT USAGE, SELECT ON SEQUENCE system_state_id_seq TO crystal_forge;
```
