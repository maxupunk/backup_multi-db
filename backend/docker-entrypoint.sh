#!/bin/bash
set -e

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
BACKUP_DIR="${BACKUP_STORAGE_PATH:-/app_data/backups}"
if [ ! -d "$BACKUP_DIR" ]; then
    echo -e "${YELLOW}📁 Criando diretório de backups: $BACKUP_DIR${NC}"
    mkdir -p "$BACKUP_DIR"
fi

# Verificar diretório do SQLite
SQLITE_DB_PATH="${SQLITE_DATABASE_PATH:-/app_data/database/app.sqlite3}"
SQLITE_DIR="$(dirname "$SQLITE_DB_PATH")"
if [ ! -d "$SQLITE_DIR" ]; then
    echo -e "${YELLOW}📁 Criando diretório do SQLite: $SQLITE_DIR${NC}"
    mkdir -p "$SQLITE_DIR"
fi

# Executar migrations
echo -e "${YELLOW}🔄 Executando migrations...${NC}"
node ace migration:run --force

echo -e "${GREEN}✅ Migrations executadas com sucesso!${NC}"

# Determinar modo de execução
if [ "$NODE_ENV" = "production" ]; then
    echo -e "${GREEN}🏭 Iniciando em modo PRODUÇÃO com Node.js...${NC}"
    exec node bin/server.js
else
    echo -e "${YELLOW}🔧 Iniciando em modo DESENVOLVIMENTO com HMR...${NC}"
    exec node ace serve --hmr
fi
