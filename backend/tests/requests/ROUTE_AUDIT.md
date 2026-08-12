# Auditoria de rotas de request

Inventário feito em 2026-08-12 com `cargo loco routes`. O router atual expõe
**89** rotas sob `/api` — não as 73 do roadmap, que eram o inventário anterior
à entrada do Docker Manager, retenção, diagnósticos e SSE próprio.

Uma rota conta como coberta somente quando há um teste de request que executa o
método e o template em questão. Um teste de autenticação que só recebe `401`
também conta: ele confirma que a rota está montada e protegida. Testes de model
ou DTO não contam nesta planilha.

| Recurso | Rotas cobertas | Sem teste direto | Dono atual |
|---|---:|---:|---|
| audit logs | 3 | 0 | `audit_logs.rs` |
| auth | 7 | 0 | `auth.rs` |
| backups | 6 | 0 | `backups.rs` |
| connections | 11 | 1 | `connections.rs`, `backups.rs` |
| docker | 9 | 17 | `docker.rs` |
| events | 1 | 0 | `events.rs` |
| health | 0 | 1 | — |
| stats/system | 3 | 7 | `system.rs`, `docker.rs` |
| storage destinations | 7 | 1 | `storages.rs` |
| storages | 13 | 0 | `storages.rs` |
| users | 2 | 0 | `users.rs` |
| **Total** | **62** | **27** | |

## Rotas sem teste direto

| Método | Rota | Arquivo que deve receber o teste |
|---|---|---|
| PATCH | `/api/connections/{id}` | `connections.rs` |
| GET | `/api/docker/containers/{id}` | `docker.rs` |
| DELETE | `/api/docker/containers/{id}` | `docker.rs` |
| GET | `/api/docker/containers/{id}/logs` | `docker.rs` |
| DELETE | `/api/docker/containers/{id}/logs` | `docker.rs` |
| POST | `/api/docker/containers/{id}/restart` | `docker.rs` |
| POST | `/api/docker/containers/{id}/start` | `docker.rs` |
| POST | `/api/docker/containers/{id}/stop` | `docker.rs` |
| GET | `/api/docker/diagnostics/{id}` | `docker.rs` |
| GET | `/api/docker/environment` | `docker.rs` |
| POST | `/api/docker/images/prune` | `docker.rs` |
| GET | `/api/docker/images/{id}` | `docker.rs` |
| DELETE | `/api/docker/images/{id}` | `docker.rs` |
| POST | `/api/docker/networks` | `docker.rs` |
| GET | `/api/docker/networks/{id}` | `docker.rs` |
| POST | `/api/docker/networks/{id}/connect` | `docker.rs` |
| POST | `/api/docker/networks/{id}/disconnect` | `docker.rs` |
| GET | `/api/docker/volumes/{name}` | `docker.rs` |
| GET | `/api/health` | `public.rs` ou `system.rs` |
| PATCH | `/api/storage-destinations/{id}` | `storages.rs` |
| GET | `/api/system/backup-retention` | `system.rs` |
| PUT | `/api/system/backup-retention` | `system.rs` |
| POST | `/api/system/backup-retention/run` | `system.rs` |
| GET | `/api/system/diagnostics` | `system.rs` |
| DELETE | `/api/system/diagnostics/{name}` | `system.rs` |
| GET | `/api/system/diagnostics/{name}/download` | `system.rs` |
| GET | `/api/system/resources/history` | `system.rs` |

`GET /api/system/containers/resources` é exercitada em `docker.rs`; ela entra
no grupo system, embora use dados Docker. As listagens de containers, volumes,
networks e imagens também são cobertas pelo teste de degradação quando a Engine
não está disponível; as operações específicas da tabela acima não são.

## Como refazer

1. Rode `cargo loco routes` dentro de `backend/` e conte somente linhas
   `METHOD /api/...`.
2. Procure cada template em `tests/requests/*.rs`, incluindo os testes que
   montam a URL com `format!`.
3. Atualize os totais e mova uma rota para a primeira tabela apenas depois de
   um teste de request ter sido adicionado e executado.
