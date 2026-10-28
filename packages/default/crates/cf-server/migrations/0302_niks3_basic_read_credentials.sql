-- Basic read credentials are independent of write authentication. Existing
-- destinations remain unchanged; the server encrypts new passwords before bind.
ALTER TABLE cache_destinations
    ADD COLUMN niks3_read_basic_username TEXT,
    ADD COLUMN niks3_read_basic_password TEXT;

ALTER TABLE cache_destinations
    DROP CONSTRAINT cache_destinations_niks3_read_auth_mode_check,
    DROP CONSTRAINT cache_destinations_niks3_auth_check,
    ADD CONSTRAINT cache_destinations_niks3_read_auth_mode_check
        CHECK (niks3_read_auth_mode IN ('none', 'basic', 'mtls')),
    ADD CONSTRAINT cache_destinations_basic_type_check
        CHECK (cache_type = 'Niks3' OR
            (niks3_read_basic_username IS NULL AND niks3_read_basic_password IS NULL)),
    ADD CONSTRAINT cache_destinations_niks3_auth_check CHECK (
        cache_type <> 'Niks3' OR (
            niks3_server_url IS NOT NULL AND push_to IS NOT NULL
            AND cardinality(niks3_public_keys) > 0
            AND niks3_write_auth_mode IS NOT NULL AND niks3_read_auth_mode IS NOT NULL
            AND (
                (niks3_write_auth_mode = 'token' AND niks3_auth_token IS NOT NULL
                 AND btrim(niks3_auth_token) <> '' AND niks3_write_client_cert IS NULL
                 AND niks3_write_client_key IS NULL AND niks3_write_ca_cert IS NULL)
                OR
                (niks3_write_auth_mode = 'mtls' AND niks3_auth_token IS NULL
                 AND niks3_write_client_cert IS NOT NULL AND btrim(niks3_write_client_cert) <> ''
                 AND niks3_write_client_key IS NOT NULL AND btrim(niks3_write_client_key) <> '')
            )
            AND (
                (niks3_read_auth_mode = 'none' AND niks3_read_client_cert IS NULL
                 AND niks3_read_client_key IS NULL AND niks3_read_ca_cert IS NULL
                 AND niks3_read_basic_username IS NULL AND niks3_read_basic_password IS NULL)
                OR
                (niks3_read_auth_mode = 'mtls' AND niks3_read_client_cert IS NOT NULL
                 AND btrim(niks3_read_client_cert) <> '' AND niks3_read_client_key IS NOT NULL
                 AND btrim(niks3_read_client_key) <> '' AND niks3_read_basic_username IS NULL
                 AND niks3_read_basic_password IS NULL)
                OR
                (niks3_read_auth_mode = 'basic' AND niks3_read_client_cert IS NULL
                 AND niks3_read_client_key IS NULL AND niks3_read_ca_cert IS NULL
                 AND niks3_read_basic_username IS NOT NULL AND niks3_read_basic_username <> ''
                 AND niks3_read_basic_password IS NOT NULL AND niks3_read_basic_password <> '')
            )
        )
    );

COMMENT ON COLUMN cache_destinations.niks3_read_basic_username IS
    'Server-only Basic read username. Management responses omit this field.';
COMMENT ON COLUMN cache_destinations.niks3_read_basic_password IS
    'Server-only Basic read password encrypted with the existing cache envelope.';
