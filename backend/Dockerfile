# =============================================================================
# DOCKERFILE - DB Backup Manager
# Multi-stage build com Node 25-bookworm-slim
# Inclui mysql-client e postgresql-client para mysqldump e pg_dump
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Base - Instala dependências do sistema
# -----------------------------------------------------------------------------
FROM node:25-bookworm-slim AS base

# Instalar dependências do sistema necessárias
# - mysql-client: fornece mysqldump para backups MySQL/MariaDB
# - postgresql-client: fornece pg_dump para backups PostgreSQL
# - openssl: requerido para criptografia
# - python3 + build-essential: requerido para compilar better-sqlite3
RUN apt-get update && apt-get install -y --no-install-recommends \
    default-mysql-client \
    postgresql-client \
    openssl \
    python3 \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# -----------------------------------------------------------------------------
# Stage 2: Dependencies - Instala dependências Node.js
# -----------------------------------------------------------------------------
FROM base AS dependencies

# Copiar arquivos de configuração de dependências
COPY package*.json ./

# Instalar todas as dependências (incluindo devDependencies para build)
RUN npm ci

# -----------------------------------------------------------------------------
# Stage 3: Build - Compila a aplicação
# -----------------------------------------------------------------------------
FROM dependencies AS build

# Copiar código fonte
COPY . .

# Build da aplicação
RUN node ace build

# -----------------------------------------------------------------------------
# Stage 4: Production - Imagem final otimizada
# -----------------------------------------------------------------------------
FROM base AS production

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=3333

WORKDIR /app

# Instalar PM2 globalmente
RUN npm install -g pm2

# Copiar build e package.json
COPY --from=build /app/build ./
COPY --from=build /app/package*.json ./

# Copiar configuração do PM2
COPY ecosystem.config.cjs ./

# Instalar apenas dependências de produção
RUN npm ci --only=production && npm cache clean --force

# Copiar e configurar entrypoint
COPY docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

# Criar diretórios necessários (incluindo logs)
RUN mkdir -p /app/storage/backups /app/storage/database /app/logs

# Criar usuário não-root para segurança
RUN groupadd -r appgroup && useradd -r -g appgroup -m appuser \
    && chown -R appuser:appgroup /app

# Criar diretório PM2 para o usuário
RUN mkdir -p /home/appuser/.pm2 \
    && chown -R appuser:appgroup /home/appuser/.pm2

USER appuser

EXPOSE 3333

HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD node -e "fetch('http://localhost:3333/health').then(r => r.ok ? process.exit(0) : process.exit(1)).catch(() => process.exit(1))"

ENTRYPOINT ["/docker-entrypoint.sh"]

# -----------------------------------------------------------------------------
# Stage 5: Development - Imagem para desenvolvimento
# -----------------------------------------------------------------------------
FROM dependencies AS development

ENV NODE_ENV=development
ENV HOST=0.0.0.0
ENV PORT=3333

WORKDIR /app

# Copiar código fonte
COPY . .

# Copiar e configurar entrypoint
COPY docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

# Criar diretórios necessários
RUN mkdir -p /app/storage/backups /app/storage/database

EXPOSE 3333

ENTRYPOINT ["/docker-entrypoint.sh"]
