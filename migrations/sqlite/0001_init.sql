CREATE TABLE users (
    id BLOB NOT NULL PRIMARY KEY,
    email TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    global_role TEXT NOT NULL DEFAULT 'user' CHECK (global_role IN ('owner', 'user')),
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX idx_users_email_lower ON users (LOWER(email));

CREATE TABLE tenants (
    id BLOB NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    created_by BLOB REFERENCES users(id) ON DELETE SET NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tenant_members (
    tenant_id BLOB NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('tenant_admin', 'viewer')),
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, user_id)
);
CREATE INDEX idx_tenant_members_user ON tenant_members (user_id);

CREATE TABLE admin_sessions (
    id BLOB NOT NULL PRIMARY KEY,
    user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE providers (
    id BLOB NOT NULL PRIMARY KEY,
    tenant_id BLOB NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    kind TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key_env TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    timeout_ms INTEGER NOT NULL DEFAULT 60000,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (tenant_id, code)
);

CREATE TABLE model_routes (
    id BLOB NOT NULL PRIMARY KEY,
    tenant_id BLOB NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    provider_code TEXT NOT NULL,
    external_model TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (tenant_id, alias),
    FOREIGN KEY (tenant_id, provider_code) REFERENCES providers(tenant_id, code) ON DELETE RESTRICT
);

CREATE TABLE provider_secrets (
    id BLOB NOT NULL PRIMARY KEY,
    tenant_id BLOB NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    provider_id BLOB NOT NULL UNIQUE REFERENCES providers(id) ON DELETE CASCADE,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    algorithm TEXT NOT NULL,
    key_version INTEGER NOT NULL,
    created_by BLOB REFERENCES users(id) ON DELETE SET NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_provider_secrets_tenant_provider ON provider_secrets (tenant_id, provider_id);

CREATE TABLE gateway_clients (
    id BLOB NOT NULL PRIMARY KEY,
    tenant_id BLOB NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_used_at DATETIME,
    created_by BLOB REFERENCES users(id) ON DELETE SET NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (tenant_id, name)
);

CREATE TABLE requests (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id BLOB NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    gateway_client_id BLOB REFERENCES gateway_clients(id) ON DELETE SET NULL,
    created_at DATETIME NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    model_alias TEXT NOT NULL,
    provider_code TEXT,
    external_model TEXT,
    status TEXT NOT NULL,
    latency_ms INTEGER,
    error_message TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    prompt_preview TEXT,
    response_preview TEXT
);
CREATE INDEX idx_requests_created_at_desc ON requests (created_at DESC);
CREATE INDEX idx_requests_model_alias ON requests (model_alias);
CREATE INDEX idx_requests_status ON requests (status);
CREATE INDEX idx_requests_tenant_created_at_desc ON requests (tenant_id, created_at DESC);
CREATE INDEX idx_requests_gateway_client ON requests (gateway_client_id);
