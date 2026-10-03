-- Niks3 separates the write API from the Nix read URL stored in push_to.
-- Tokens and private keys contain application-encrypted AES-256-GCM envelopes.
ALTER TABLE cache_destinations
    DROP CONSTRAINT cache_destinations_cache_type_check,
    ADD CONSTRAINT cache_destinations_cache_type_check
        CHECK (cache_type IN ('S3', 'Attic', 'Http', 'Nix', 'Niks3')),
    ADD COLUMN niks3_server_url TEXT,
    ADD COLUMN niks3_public_keys TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN niks3_write_auth_mode TEXT
        CHECK (niks3_write_auth_mode IN ('token', 'mtls')),
    ADD COLUMN niks3_auth_token TEXT,
    ADD COLUMN niks3_write_client_cert TEXT,
    ADD COLUMN niks3_write_client_key TEXT,
    ADD COLUMN niks3_write_ca_cert TEXT,
    ADD COLUMN niks3_read_auth_mode TEXT
        CHECK (niks3_read_auth_mode IN ('none', 'mtls')),
    ADD COLUMN niks3_read_client_cert TEXT,
    ADD COLUMN niks3_read_client_key TEXT,
    ADD COLUMN niks3_read_ca_cert TEXT,
    ADD CONSTRAINT cache_destinations_niks3_auth_check CHECK (
        cache_type <> 'Niks3' OR (
            niks3_server_url IS NOT NULL AND push_to IS NOT NULL
            AND cardinality(niks3_public_keys) > 0
            AND niks3_write_auth_mode IS NOT NULL
            AND niks3_read_auth_mode IS NOT NULL
            AND (
                (niks3_write_auth_mode = 'token'
                 AND niks3_auth_token IS NOT NULL AND btrim(niks3_auth_token) <> ''
                 AND niks3_write_client_cert IS NULL
                 AND niks3_write_client_key IS NULL AND niks3_write_ca_cert IS NULL)
                OR
                (niks3_write_auth_mode = 'mtls' AND niks3_auth_token IS NULL
                 AND niks3_write_client_cert IS NOT NULL AND btrim(niks3_write_client_cert) <> ''
                 AND niks3_write_client_key IS NOT NULL AND btrim(niks3_write_client_key) <> '')
            )
            AND (
                (niks3_read_auth_mode = 'none' AND niks3_read_client_cert IS NULL
                 AND niks3_read_client_key IS NULL AND niks3_read_ca_cert IS NULL)
                OR
                (niks3_read_auth_mode = 'mtls'
                 AND niks3_read_client_cert IS NOT NULL AND btrim(niks3_read_client_cert) <> ''
                 AND niks3_read_client_key IS NOT NULL AND btrim(niks3_read_client_key) <> '')
            )
        )
    );
