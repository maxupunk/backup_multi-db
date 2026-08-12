# Backend — Rust/Loco do DB Backup Manager

Este diretório contém o backend em **Rust** com o framework [Loco](https://loco.rs).

## Objetivo

Backend principal do DB Backup Manager, servindo a API REST e os assets do frontend Vue.

## Requisitos

- Rust **1.96** ou superior
- Node.js **26** (apenas para build do frontend dentro do Dockerfile)
- SQLite 3
- (Opcional) Docker e Docker Compose para deploy

## Configuração

O Loco lê `config/{development,test,production}.yaml`. Variáveis sensíveis são injetadas via ambiente usando o helper `get_env` do YAML — nenhum segredo fica versionado.

Variáveis obrigatórias em qualquer ambiente:

```sh
DB_ENCRYPTION_KEY=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
```

A chave deve ter **64 caracteres hex** (32 bytes).

Outras variáveis comuns:

```sh
LOCO_ENV=development
PORT=5150
BINDING=127.0.0.1
DATABASE_URL=sqlite://backend_development.sqlite?mode=rwc
INITIAL_ADMIN_BOOTSTRAP_TOKEN=seu-token-de-bootstrap
JWT_SECRET=<base64; obrigatório em produção>
JWT_EXPIRATION=604800
BACKUP_STORAGE_PATH=/storage/backups
DIAGNOSTICS_PATH=/storage/diagnostics
AUDIT_RETENTION_DAYS=30
```

## Executando localmente

```sh
cd backend

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

## Frontend

O frontend Vue em `frontend/` builda sem alterações e pode ser servido pelo backend via fallback SPA (`app.rs::after_routes`). Para testar localmente:

```sh
cd frontend
pnpm build                        # gera dist/
cp -r dist/* ../backend/public/
cd ../backend
cargo loco start                  # / serve o SPA; /api/* permanecem inalterados
```

Em desenvolvimento, o Vite proxy ainda aponta para `VITE_BACKEND_URL` (padrão `http://localhost:3333`); ajuste para a porta do backend (`5150`) se quiser testar o modo dev contra o Rust.

## Build de produção (Docker)

O Dockerfile multi-stage builda o frontend Vue e o backend Rust, produzindo uma imagem enxuta baseada em `debian:trixie-slim`:

```sh
docker build -f backend/Dockerfile -t backend:latest .
```

O `docker-compose.yml` na raiz sobe o backend em produção com volume para SQLite, backups e diagnósticos:

```sh
docker compose up -d backend
```
