CREATE TABLE accounts (
    id             BIGSERIAL PRIMARY KEY,
    name           TEXT        NOT NULL,
    api_key_hash   TEXT        NOT NULL UNIQUE,
    balance_kes    BIGINT      NOT NULL DEFAULT 0 CHECK (balance_kes >= 0),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE usage_records (
    id            BIGSERIAL PRIMARY KEY,
    account_id    BIGINT      NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    operations    TEXT        NOT NULL,
    input_bytes   BIGINT      NOT NULL,
    output_bytes  BIGINT      NOT NULL,
    cost_kes      BIGINT      NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX usage_records_account_created
    ON usage_records (account_id, created_at DESC);