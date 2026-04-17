FROM node:22-bookworm-slim AS admin-build
WORKDIR /app/admin
COPY admin/package.json admin/package-lock.json ./
RUN npm ci
COPY admin/ ./
RUN npm run build

FROM rust:1.91-bookworm AS backend-build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM postgres:17-bookworm AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-build /app/target/release/ai-gateway /app/ai-gateway
COPY --from=admin-build /app/admin/dist /app/admin/dist
COPY docker/entrypoint.sh /app/entrypoint.sh
COPY docker/postgresql.conf /etc/postgresql/postgresql.conf

RUN chmod +x /app/entrypoint.sh

ENV APP_HOST=0.0.0.0 \
    APP_PORT=8080 \
    ADMIN_DIST_DIR=/app/admin/dist \
    POSTGRES_USER=ai_gateway \
    POSTGRES_PASSWORD=ai_gateway \
    POSTGRES_DB=ai_gateway \
    DATABASE_URL=postgres://ai_gateway:ai_gateway@127.0.0.1:5432/ai_gateway

EXPOSE 8080 5432

ENTRYPOINT ["/app/entrypoint.sh"]
