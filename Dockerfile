# =============================================================================
# DOCKERFILE - DB Backup Manager
# =============================================================================
# Build context: projeto raiz (..)
# Stages:
#   base           — imagem base com deps do sistema (mysql/pg clients, build tools)
#   frontend-builder — compila o frontend Vue
#   dependencies   — instala node_modules com npm ci (binários Linux)
#   build          — compila a aplicação TypeScript
#   production     — imagem final enxuta com apenas deps de produção
#   development    — imagem dev com todos os node_modules compilados para Linux
# =============================================================================

# -----------------------------------------------------------------------------
# Stage: base
# Imagem base com dependências do SO necessárias em todos os stages.
# python3 + build-essential são necessários para compilar better-sqlite3.
# -----------------------------------------------------------------------------
FROM node:25.8.1-trixie AS base

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gnupg \
        && install -d /usr/share/postgresql-common/pgdg \
        && curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
            | gpg --dearmor -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.gpg \
        && . /etc/os-release \
        && echo "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.gpg] https://apt.postgresql.org/pub/repos/apt ${VERSION_CODENAME}-pgdg main" \
            > /etc/apt/sources.list.d/pgdg.list \
        && apt-get update && apt-get install -y --no-install-recommends \
        default-mysql-client \
        postgresql-client-18 \
        openssl \
        python3 \
        build-essential \
        && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# -----------------------------------------------------------------------------
# Stage: frontend-builder
# Compila o frontend Vue/Vite. Roda em paralelo com o pipeline do backend.
# -----------------------------------------------------------------------------
FROM node:25.8.1-trixie AS frontend-builder

WORKDIR /app-frontend

COPY frontend/package*.json ./
RUN npm ci

COPY frontend/ .
RUN npm run build-only -- --outDir dist

# -----------------------------------------------------------------------------
# Stage: dependencies
# Instala TODAS as dependências Node.js (incluindo devDeps para o build).
# Os binários nativos (better-sqlite3) são compilados aqui para Linux/amd64.
# Este stage é reutilizado pelos stages development e build via cache.
# -----------------------------------------------------------------------------
FROM base AS dependencies

# Copiar apenas os manifestos primeiro para aproveitar o cache do Docker:
# se package.json/package-lock.json não mudaram, o npm ci não é re-executado.
COPY backend/package*.json ./

RUN npm ci

# -----------------------------------------------------------------------------
# Stage: build
# Compila TypeScript → JavaScript. Roda em cima do stage dependencies.
# -----------------------------------------------------------------------------
FROM dependencies AS build

# Código fonte do backend (node_modules excluído via .dockerignore)
COPY backend/ .

# Build do frontend embutido no backend (pasta public)
COPY --from=frontend-builder /app-frontend/dist ./public

RUN node ace build

# -----------------------------------------------------------------------------
# Stage: production
# Imagem final enxuta: apenas o build compilado + deps de produção.
# Roda como usuário não-root (appuser) por segurança.
# -----------------------------------------------------------------------------
FROM base AS production

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=3333

RUN npm install -g pm2

# Copiar apenas o build compilado e os manifestos de dependências
COPY --from=build /app/build ./
COPY --from=build /app/package*.json ./
COPY backend/ecosystem.config.cjs ./
COPY backend/docker-entrypoint.sh /docker-entrypoint.sh

RUN sed -i 's/\r$//' /docker-entrypoint.sh \
    && chmod +x /docker-entrypoint.sh

# Instalar apenas dependências de produção (recompila módulos nativos)
RUN npm ci --only=production && npm cache clean --force

# Diretórios de dados e logs
RUN mkdir -p /app/storage/backups /app/storage/database /app/logs /app_data/backups /app_data/database

# Usuário não-root
RUN groupadd -r appgroup && useradd -r -g appgroup -m appuser \
    && chown -R appuser:appgroup /app \
    && chown -R appuser:appgroup /app_data \
    && mkdir -p /home/appuser/.pm2 \
    && chown -R appuser:appgroup /home/appuser/.pm2

USER appuser

EXPOSE 3333

HEALTHCHECK --interval=30s --timeout=10s --start-period=15s --retries=3 \
    CMD node -e "fetch('http://localhost:3333/health').then(r => r.ok ? process.exit(0) : process.exit(1)).catch(() => process.exit(1))"

ENTRYPOINT ["/docker-entrypoint.sh"]

# -----------------------------------------------------------------------------
# Stage: development
# Imagem para desenvolvimento local com hot-reload.
#
# Estratégia de volumes (definida no docker-compose.dev.yml):
#   - Bind mount .:/app          → código do host (hot-reload)
#   - Named volume node_modules  → /app/node_modules (binários Linux do container)
#
# O named volume é inicializado a partir desta imagem na primeira criação,
# garantindo que os binários Linux (compilados no stage dependencies)
# sejam usados — nunca os binários do host (Windows/Mac).
# -----------------------------------------------------------------------------
FROM dependencies AS development

ENV NODE_ENV=development
ENV HOST=0.0.0.0
ENV PORT=3333

# Código fonte copiado para a imagem (será sobrescrito pelo bind mount em dev,
# mas é necessário para o container funcionar sem volumes montados).
# node_modules NÃO são copiados (excluídos no .dockerignore) — vêm do stage
# dependencies herdado, compilados corretamente para Linux.
COPY backend/ .

COPY backend/docker-entrypoint.sh /docker-entrypoint.sh
RUN sed -i 's/\r$//' /docker-entrypoint.sh \
    && chmod +x /docker-entrypoint.sh

RUN mkdir -p /app/storage/backups /app/storage/database /app_data/backups /app_data/database

EXPOSE 3333

ENTRYPOINT ["/docker-entrypoint.sh"]

FROM production AS final
