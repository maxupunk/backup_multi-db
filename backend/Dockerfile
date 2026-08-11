# =============================================================================
# DOCKERFILE — backend (Rust/Loco)
# =============================================================================
# Build context: raiz do repositório (..), para que o compose na raiz possa
# construir esta imagem sem trocar de contexto.
#
# Stages:
#   chef      — cargo-chef, para cachear a compilação das dependências
#   planner   — extrai a "receita" de dependências do workspace
#   builder   — compila as dependências (cacheado) e só depois o código
#   runtime   — imagem final enxuta, com os clientes de banco necessários
#
# Por que cargo-chef: sem ele, tocar em um único arquivo `.rs` invalida a
# camada Docker que contém as dependências e o build recompila as ~400 crates
# do zero — minutos a cada alteração. Com ele, a camada de dependências só
# muda quando o `Cargo.toml`/`Cargo.lock` muda.
# =============================================================================

ARG RUST_IMAGE=rust:1.96-trixie
ARG RUNTIME_IMAGE=debian:trixie-slim
ARG NODE_IMAGE=node:26.0.0-trixie-slim

# -----------------------------------------------------------------------------
# Stage: frontend-builder
# Compila o frontend Vue/Vite. O resultado (`dist/`) vira `public/` do Loco,
# servido pelo fallback da SPA (Fase 12.4).
# -----------------------------------------------------------------------------
FROM ${NODE_IMAGE} AS frontend-builder

WORKDIR /app-frontend

COPY frontend/package*.json ./
RUN npm ci

COPY frontend/ .
RUN npm run build-only -- --outDir dist

# -----------------------------------------------------------------------------
# Stage: chef
# -----------------------------------------------------------------------------
FROM ${RUST_IMAGE} AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# -----------------------------------------------------------------------------
# Stage: planner
# Gera a receita a partir dos manifestos. Só o que descreve dependências entra
# aqui — é o que mantém o cache válido entre alterações de código.
# -----------------------------------------------------------------------------
FROM chef AS planner
COPY backend/ .
RUN cargo chef prepare --recipe-path recipe.json

# -----------------------------------------------------------------------------
# Stage: builder
# -----------------------------------------------------------------------------
FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json
# Compila SÓ as dependências. Esta camada é a cara e a que queremos cachear.
RUN cargo chef cook --release --recipe-path recipe.json

COPY backend/ .
# A spec OpenAPI compartilhada fica na raiz do repo; o controller a embute no
# binario via `include_str!("../../../docs/openapi-baseline.yml")`.
COPY docs /docs
# O build do Vue embutido na imagem; o fallback `GET /*` o servira'.
COPY --from=frontend-builder /app-frontend/dist ./public
RUN cargo build --release --bin backend-cli

# -----------------------------------------------------------------------------
# Stage: runtime
# A aplicação orquestra dumps de MySQL/MariaDB/PostgreSQL, então os clientes
# precisam estar na imagem — sem eles, `POST /api/connections/:id/backup`
# falha em tempo de execução com "command not found", que é o pior momento
# possível para descobrir uma dependência faltando.
# -----------------------------------------------------------------------------
FROM ${RUNTIME_IMAGE} AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        default-mysql-client \
        gnupg \
        postgresql-client \
        rclone \
    && rm -rf /var/lib/apt/lists/*

# Usuário sem privilégios: a aplicação não precisa de root, e um processo que
# monta comandos de dump é exatamente o que não deveria rodar como root.
RUN useradd --create-home --shell /usr/sbin/nologin roco

WORKDIR /app

COPY --from=builder /app/target/release/backend-cli /usr/local/bin/backend
COPY --chown=roco:roco backend/config ./config
COPY --chown=roco:roco backend/src/fixtures ./src/fixtures
COPY --chown=roco:roco --from=builder /app/public ./public

RUN mkdir -p /storage/backups /storage/database /storage/diagnostics \
    && chown -R roco:roco /storage /app

USER roco

ENV LOCO_ENV=production \
    BINDING=0.0.0.0 \
    PORT=3333

EXPOSE 3333

HEALTHCHECK --interval=30s --timeout=10s --retries=3 --start-period=10s \
    CMD curl -fsS http://localhost:3333/api/health || exit 1

CMD ["backend", "start"]
