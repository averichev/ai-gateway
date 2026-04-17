CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key_env TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    timeout_ms INTEGER NOT NULL DEFAULT 60000,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS model_routes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    alias TEXT NOT NULL UNIQUE,
    provider_code TEXT NOT NULL REFERENCES providers(code) ON DELETE RESTRICT,
    external_model TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS requests (
    id TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    model_alias TEXT NOT NULL,
    provider_code TEXT NULL REFERENCES providers(code) ON DELETE SET NULL,
    external_model TEXT NULL,
    status TEXT NOT NULL,
    latency_ms BIGINT NULL,
    error_message TEXT NULL,
    input_tokens INTEGER NULL,
    output_tokens INTEGER NULL,
    prompt_preview TEXT NULL,
    response_preview TEXT NULL
);

CREATE INDEX IF NOT EXISTS idx_requests_created_at_desc
    ON requests (created_at DESC);

CREATE INDEX IF NOT EXISTS idx_requests_model_alias
    ON requests (model_alias);

CREATE INDEX IF NOT EXISTS idx_requests_status
    ON requests (status);
