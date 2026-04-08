# Project Guidelines — backup_multi-db

## Stack

- **Backend**: AdonisJS v6 (TypeScript, ESM), pnpm, Japa tests
- **Frontend**: Vue 3 + Vite, pnpm
- **Infra**: Docker, docker-compose

## Backend — Quality Gates

After every code change in `backend/`, always run these three commands in order and fix any failures before considering the task done:

```bash
cd backend
pnpm typecheck   # tsc --noEmit — zero TypeScript errors required
pnpm lint        # ESLint — zero lint errors required
pnpm test        # Japa functional tests
```

If running inside the container use `node ace` equivalents:
- `node ace build` (includes typecheck)
- `node ace test`

## Architecture & SOLID

- **Single Responsibility**: one class/service per concern. Controllers only validate input and delegate to services.
- **Open/Closed**: extend behavior through new adapters (see `StorageExplorerAdapter`) rather than modifying existing ones.
- **Liskov Substitution**: adapter implementations must satisfy the interface contract fully.
- **Interface Segregation**: keep interfaces narrow (see `storage/types.ts`).
- **Dependency Inversion**: inject dependencies via constructor or AdonisJS IoC; never hard-code concrete classes.

Services live in `app/services/`. Domain logic must NOT live in controllers or models.

## Path Aliases

Backend uses `#services/*`, `#models/*`, `#controllers/*`, etc. — defined in `backend/package.json` `imports` field. Always use aliases, never relative paths across feature boundaries.

## Testing

- Tests are in `backend/tests/functional/`.
- Use the Japa `apiClient` for HTTP tests.
- See `backend/tests/bootstrap.ts` for setup conventions.

## Git Hygiene

- Never commit secrets or `.env` files.
- Keep `backend/.gitignore` specific: use `/storage` (leading slash) to only ignore the top-level `storage/` directory — not `app/services/storage/`.
- Stage and commit new files before building the Docker image.
