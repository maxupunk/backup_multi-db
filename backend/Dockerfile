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
# O `prepare` nao copia os manifestos: ele os re-serializa para dentro da
# receita, e nesse round-trip carimba `edition` em cada alvo ([lib], [[bin]],
# [[test]], [[example]]). O Cargo 1.9x deprecou a chave nesse nivel, entao o
# `cook` do stage seguinte imprimiria um bloco de warnings que nao vem de
# nenhum Cargo.toml deste repositorio. Ver o cabecalho do script e o upstream
# LukeMathWalker/cargo-chef#350.
RUN cargo chef prepare --recipe-path recipe.json \
    && python3 docker/strip-target-edition.py recipe.json

# -----------------------------------------------------------------------------
# Stage: builder
# -----------------------------------------------------------------------------
FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json
# Compila SÓ as dependências. Esta camada é a cara e a que queremos cachear.
RUN cargo chef cook --release --recipe-path recipe.json

COPY backend/ .
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

# O cliente PostgreSQL vem do PGDG, nao do Debian. O trixie congela o
# `postgresql-client` no 17, e o `pg_dump` recusa por design qualquer servidor
# mais novo que ele ("aborting because of server version mismatch"). Como esta
# aplicacao faz dump de bancos de terceiros, a versao do servidor nao esta sob
# nosso controle — o cliente e' que precisa acompanhar.
#
# A regra e' unidirecional: `pg_dump` N faz dump de servidor <= N. Por isso
# instalamos **um** cliente, o mais novo, e nao um leque de versoes com escolha
# em tempo de execucao: um cliente antigo nao acrescenta alcance nenhum, so'
# ocupa imagem e um caminho de codigo a mais para manter.
#
# Quando sair uma major nova do PostgreSQL, bumpar `PG_MAJOR` e' a unica
# alteracao necessaria — a 19 tem GA prevista para set/out de 2026.
ARG PG_MAJOR=18

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && install -d /usr/share/postgresql-common/pgdg \
    && curl -fsSL -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc \
        https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    && echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt $(. /etc/os-release && echo "$VERSION_CODENAME")-pgdg main" > /etc/apt/sources.list.d/pgdg.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        default-mysql-client \
        postgresql-client-${PG_MAJOR} \
        rclone \
    && rm -rf /var/lib/apt/lists/*

# O `/usr/bin/pg_dump` do Debian nao e' o binario: e' o `pg_wrapper`, que
# escolhe a versao por heuristica de cluster local. Aqui nao ha' cluster local
# — todo destino e' remoto — e uma heuristica nao e' onde se quer apoiar a
# escolha do binario que produz o backup. Apontar o PATH para os binarios reais
# torna a resolucao deterministica e explicita.
ENV PATH="/usr/lib/postgresql/${PG_MAJOR}/bin:${PATH}"

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
    LOCO_CONFIG_FOLDER=/app/config \
    BINDING=0.0.0.0 \
    PORT=3333

EXPOSE 3333

HEALTHCHECK --interval=30s --timeout=10s --retries=3 --start-period=10s \
    CMD curl -fsS http://localhost:3333/api/health || exit 1

CMD ["backend", "start"]
