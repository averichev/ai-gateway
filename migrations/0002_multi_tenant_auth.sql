CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    global_role TEXT NOT NULL DEFAULT 'user' CHECK (global_role IN ('owner', 'user')),
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_lower
    ON users (LOWER(email));

CREATE TABLE IF NOT EXISTS tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    created_by UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO tenants (name, slug)
VALUES ('Default tenant', 'default')
ON CONFLICT (slug) DO NOTHING;

CREATE TABLE IF NOT EXISTS tenant_members (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('tenant_admin', 'viewer')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, user_id)
);

CREATE TABLE IF NOT EXISTS admin_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE providers
    ADD COLUMN IF NOT EXISTS tenant_id UUID NULL;

UPDATE providers
SET tenant_id = (SELECT id FROM tenants WHERE slug = 'default')
WHERE tenant_id IS NULL;

ALTER TABLE providers
    ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE providers
    ADD CONSTRAINT providers_tenant_id_fkey
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE model_routes
    ADD COLUMN IF NOT EXISTS tenant_id UUID NULL;

UPDATE model_routes
SET tenant_id = (SELECT id FROM tenants WHERE slug = 'default')
WHERE tenant_id IS NULL;

ALTER TABLE model_routes
    ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE requests
    ADD COLUMN IF NOT EXISTS tenant_id UUID NULL,
    ADD COLUMN IF NOT EXISTS gateway_client_id UUID NULL;

UPDATE requests
SET tenant_id = (SELECT id FROM tenants WHERE slug = 'default')
WHERE tenant_id IS NULL;

ALTER TABLE requests
    ALTER COLUMN tenant_id SET NOT NULL;

ALTER TABLE model_routes
    DROP CONSTRAINT IF EXISTS model_routes_provider_code_fkey;

ALTER TABLE requests
    DROP CONSTRAINT IF EXISTS requests_provider_code_fkey;

ALTER TABLE model_routes
    DROP CONSTRAINT IF EXISTS model_routes_alias_key;

ALTER TABLE providers
    DROP CONSTRAINT IF EXISTS providers_code_key;

ALTER TABLE providers
    ADD CONSTRAINT providers_tenant_code_key UNIQUE (tenant_id, code);

ALTER TABLE model_routes
    ADD CONSTRAINT model_routes_tenant_alias_key UNIQUE (tenant_id, alias);

ALTER TABLE model_routes
    ADD CONSTRAINT model_routes_tenant_provider_code_fkey
    FOREIGN KEY (tenant_id, provider_code)
    REFERENCES providers(tenant_id, code)
    ON DELETE RESTRICT;

CREATE TABLE IF NOT EXISTS gateway_clients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_used_at TIMESTAMPTZ NULL,
    created_by UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, name)
);

ALTER TABLE requests
    ADD CONSTRAINT requests_tenant_id_fkey
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE requests
    ADD CONSTRAINT requests_gateway_client_id_fkey
    FOREIGN KEY (gateway_client_id) REFERENCES gateway_clients(id) ON DELETE SET NULL;

CREATE TABLE IF NOT EXISTS provider_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    provider_id UUID NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    ciphertext BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    algorithm TEXT NOT NULL,
    key_version INTEGER NOT NULL,
    created_by UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (provider_id)
);

CREATE INDEX IF NOT EXISTS idx_tenant_members_user
    ON tenant_members (user_id);

CREATE INDEX IF NOT EXISTS idx_gateway_clients_tenant
    ON gateway_clients (tenant_id);

CREATE INDEX IF NOT EXISTS idx_provider_secrets_tenant_provider
    ON provider_secrets (tenant_id, provider_id);

CREATE INDEX IF NOT EXISTS idx_model_routes_tenant_alias
    ON model_routes (tenant_id, alias);

CREATE INDEX IF NOT EXISTS idx_requests_tenant_created_at_desc
    ON requests (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_requests_gateway_client
    ON requests (gateway_client_id);
