# Project Guidelines — backup_multi-db

## Stack

- **Backend**: Rust + Loco (framework web), SeaORM, SQLite
- **Frontend**: Vue 3 + Vite, pnpm
- **Infra**: Docker, docker-compose

## Backend — Quality Gates

After every code change in `back-roco/`, always run these commands and fix any failures before considering the task done:

```bash
cd back-roco
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Architecture & SOLID

- **Single Responsibility**: one service per concern. Controllers only validate input and delegate to services.
- **Open/Closed**: extend behavior through new adapters rather than modifying existing ones.
- **Liskov Substitution**: adapter implementations must satisfy the interface contract fully.
- **Interface Segregation**: keep interfaces narrow.
- **Dependency Injection**: use Loco's DI container and constructor injection.

Domain logic must NOT live in controllers or models.

## Testing

- Backend tests are in `back-roco/tests/`.
- Contract tests are in `contract-tests/` and run black-box against the back-roco HTTP API.

## Git Hygiene

- Never commit secrets or `.env` files.
- Keep `back-roco/.gitignore` specific: use `/storage` (leading slash) to only ignore the top-level `storage/` directory.
- Stage and commit new files before building the Docker image.
