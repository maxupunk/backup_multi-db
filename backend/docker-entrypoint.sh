#!/bin/bash
set -e

# shellcheck source=./docker-memory.sh
. /docker-memory.sh

echo "🚀 Starting DB Backup Manager..."

# Cores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Verificar se as ferramentas de backup estão disponíveis
echo -e "${YELLOW}📦 Verificando ferramentas de backup...${NC}"

if command -v mysqldump &> /dev/null; then
    echo -e "${GREEN}✅ mysqldump disponível: $(mysqldump --version | head -n1)${NC}"
else
    echo -e "${RED}⚠️ mysqldump não encontrado - backups MySQL/MariaDB não funcionarão${NC}"
fi

if command -v pg_dump &> /dev/null; then
    echo -e "${GREEN}✅ pg_dump disponível: $(pg_dump --version)${NC}"
else
    echo -e "${RED}⚠️ pg_dump não encontrado - backups PostgreSQL não funcionarão${NC}"
fi

# Verificar diretório de backups
BACKUP_DIR="${BACKUP_STORAGE_PATH:-/storage/backups}"
if [ ! -d "$BACKUP_DIR" ]; then
    echo -e "${YELLOW}📁 Criando diretório de backups: $BACKUP_DIR${NC}"
    mkdir -p "$BACKUP_DIR"
fi

# Verificar diretório do SQLite
SQLITE_DB_PATH="${SQLITE_DATABASE_PATH:-/storage/database/app.sqlite3}"
SQLITE_DIR="$(dirname "$SQLITE_DB_PATH")"
if [ ! -d "$SQLITE_DIR" ]; then
    echo -e "${YELLOW}📁 Criando diretório do SQLite: $SQLITE_DIR${NC}"
    mkdir -p "$SQLITE_DIR"
fi

# Diretório de artefatos de diagnóstico (heap snapshots)
DIAGNOSTICS_DIR="${DIAGNOSTICS_PATH:-/storage/diagnostics}"
if [ ! -d "$DIAGNOSTICS_DIR" ]; then
    mkdir -p "$DIAGNOSTICS_DIR" 2>/dev/null || true
fi

# Executar migrations
echo -e "${YELLOW}🔄 Executando migrations...${NC}"
node ace migration:run --force

echo -e "${GREEN}✅ Migrations executadas com sucesso!${NC}"

# Determinar modo de execução
if [ "$NODE_ENV" = "production" ]; then
    echo -e "${GREEN}🏭 Iniciando em modo PRODUÇÃO com Node.js...${NC}"

    # --max-old-space-size derivado do limite de memória do container (~65%),
    # lido do cgroup no boot. Antes era um 320 fixo que só continuava correto
    # enquanto o limite do compose fosse 512M: mudar o limite e esquecer o heap
    # dá OOM killer (heap alto demais) ou "heap out of memory" com o container
    # meio vazio (heap baixo demais). NODE_MAX_OLD_SPACE_MB continua vencendo
    # quando informado.
    configure_max_old_space

    if [ -n "${MAX_OLD_SPACE_INVALID_OVERRIDE:-}" ]; then
        echo -e "${RED}⚠️ NODE_MAX_OLD_SPACE_MB=\"${MAX_OLD_SPACE_INVALID_OVERRIDE}\" não é um número de MB — ignorado.${NC}"
    fi

    # --heapsnapshot-near-heap-limit grava UM snapshot quando o heap se aproxima
    # do limite, transformando um exit 137 silencioso em evidência analisável.
    # --diagnostic-dir aponta para um volume: sem isso o arquivo iria para a
    # camada efêmera do container e sumiria no restart.
    echo -e "${GREEN}   heap: ${MAX_OLD_SPACE_MB}MB (${MAX_OLD_SPACE_ORIGIN}) | diagnósticos: ${DIAGNOSTICS_DIR}${NC}"

    if [ "$MAX_OLD_SPACE_MB" -lt 128 ]; then
        echo -e "${RED}⚠️ Heap de ${MAX_OLD_SPACE_MB}MB é pequeno demais para operar com folga.${NC}"
        echo -e "${RED}   Aumente o limite de memória do container ou defina NODE_MAX_OLD_SPACE_MB.${NC}"
    fi

    exec node \
        --max-old-space-size="$MAX_OLD_SPACE_MB" \
        --heapsnapshot-near-heap-limit=1 \
        --diagnostic-dir="$DIAGNOSTICS_DIR" \
        bin/server.js
else
    echo -e "${YELLOW}🔧 Iniciando em modo DESENVOLVIMENTO com HMR...${NC}"
    exec node ace serve --hmr
fi
