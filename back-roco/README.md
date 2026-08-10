# back-roco — Backend Rust/Loco do DB Backup Manager

Este diretório contém o backend reescrito em **Rust** com o framework [Loco](https://loco.rs), mantendo paridade de contrato HTTP com o backend legado AdonisJS em `backend/`.

## Objetivo

Substituir o backend Node.js/AdonisJS pelo backend Rust/Loco sem alterar o frontend nem quebrar sessões existentes. As decisões de arquitetura estão documentadas em `ROADMAP_BACK_ROCO.md` na raiz do repositório.

## Requisitos

- Rust **1.96** ou superior
- Node.js **26** (apenas para build do frontend dentro do Dockerfile)
- SQLite 3 (o Loco usa SQLite por padrão)
- (Opcional) Docker e Docker Compose para deploy

## Configuração

O Loco lê `config/{development,test,production}.yaml`. Variáveis sensíveis são injetadas via ambiente usando o helper `get_env` do YAML — nenhum segredo fica versionado.

Variáveis obrigatórias em qualquer ambiente:

```sh
DB_ENCRYPTION_KEY=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
```

A chave deve ter **64 caracteres hex** (32 bytes) e ser **idêntica** à `DB_ENCRYPTION_KEY` do backend Adonis, senão os ciphertexts já gravados ficam ilegíveis.

Outras variáveis comuns:

```sh
LOCO_ENV=development
PORT=5150
BINDING=127.0.0.1
DATABASE_URL=sqlite://back_roco_development.sqlite?mode=rwc
INITIAL_ADMIN_BOOTSTRAP_TOKEN=seu-token-de-bootstrap
AUTH_ACCESS_TOKEN_EXPIRES_IN=7d
BACKUP_STORAGE_PATH=/storage/backups
DIAGNOSTICS_PATH=/storage/diagnostics
AUDIT_RETENTION_DAYS=30
```

## Executando localmente

```sh
cd back-roco

cargo loco db migrate       # aplica migrations
cargo loco start            # sobe o servidor em development
```

O servidor escuta em `http://127.0.0.1:5150` por padrão.

## Testes

```sh
# Suíte completa (unitários + integração)
cargo test

# Formatação e lint
cargo fmt
cargo clippy --all-targets -- -D warnings
```

## Suíte de contrato

A suíte em `contract-tests/` roda os mesmos testes black-box contra o Adonis e contra o back-roco. Para rodar contra o back-roco:

```sh
cd contract-tests
pnpm install
pnpm contract:roco
```

Para rodar o diff automatizado entre as duas implementações e gerar o relatório:

```sh
pnpm contract:adonis   # Adonis vs golden files
pnpm contract:diff     # back-roco + reports/contract-diff.md
```

## Frontend

O frontend Vue em `frontend/` builda sem alterações e pode ser servido pelo back-roco via fallback SPA (`app.rs::after_routes`). Para testar localmente:

```sh
cd frontend
pnpm build                        # gera ../backend/public
cp -r ../backend/public/* ../back-roco/public/
cd ../back-roco
cargo loco start                  # / serve o SPA; /api/* permanecem inalterados
```

Em desenvolvimento, o Vite proxy ainda aponta para `VITE_BACKEND_URL` (padrão `http://localhost:3333`); ajuste para a porta do back-roco (`5150`) se quiser testar o modo dev contra o Rust.

## Build de produção (Docker)

O Dockerfile multi-stage builda o frontend Vue e o backend Rust, produzindo uma imagem enxuta baseada em `debian:trixie-slim`:

```sh
docker build -f back-roco/Dockerfile -t back-roco:latest .
```

O `docker-compose.yml` na raiz sobe o back-roco em produção com volume para SQLite, backups e diagnósticos:

```sh
docker compose up -d backend
```

## Cutover e rollback

O cutover é **big-bang** (decisão D8). O passo a passo detalhado está em `back-roco/AGENTS.md` na seção "Runbook de cutover e rollback".

Resumo:

1. **Feature freeze** no `backend/`.
2. **Snapshot** do SQLite de produção e do diretório de backups.
3. **Migrar dados** do schema Adonis para o schema back-roco usando `cargo run --bin migrate_data`.
4. **Subir o back-roco** apontando para o banco migrado.
5. **Verificar** `/api/health`, login de um usuário existente e primeiro backup.
6. **Rollback**: derrubar o back-roco, restaurar o SQLite do snapshot e subir o Adonis novamente.

Mantenha o snapshot pré-cutover até o período de estabilidade definido (N dias).
