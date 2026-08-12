# Auditoria de rotas de request

Inventário refeito em 2026-08-12 com `cargo loco routes`. O router expõe **89**
rotas sob `/api`, e todas possuem ao menos um teste de request que executa o
método e o template correspondente.

Uma rota protegida pode ser coberta pelo caso não autorizado: ele confirma tanto
o método quanto o caminho e que a autenticação é aplicada antes da operação. Os
testes de fluxo feliz e de entrada inválida permanecem nos arquivos do recurso.

| Recurso | Rotas cobertas | Sem teste direto | Arquivo |
|---|---:|---:|---|
| audit logs | 3 | 0 | `audit_logs.rs` |
| auth | 7 | 0 | `auth.rs` |
| backups | 6 | 0 | `backups.rs` |
| connections | 12 | 0 | `connections.rs` |
| docker | 26 | 0 | `docker.rs` |
| events | 1 | 0 | `events.rs` |
| health | 1 | 0 | `system.rs` |
| stats/system | 10 | 0 | `system.rs`, `docker.rs` |
| storage destinations | 8 | 0 | `storages.rs` |
| storages | 13 | 0 | `storages.rs` |
| users | 2 | 0 | `users.rs` |
| **Total** | **89** | **0** | |

## Cobertura acrescentada nesta revisão

- `PATCH /api/connections/{id}` e `PATCH /api/storage-destinations/{id}`;
- as 17 operações Docker que não tinham execução direta;
- as 7 rotas de system/retention/diagnostics que não tinham execução direta;
- `GET /api/health`, inclusive com snapshot revisado em `insta`.

## Como refazer

1. Rode `cargo loco routes` dentro de `backend/` e conte somente linhas
   `METHOD /api/...`.
2. Procure cada template em `tests/requests/*.rs`, incluindo URLs montadas com
   `format!` e os testes de autorização.
3. Atualize a tabela ao adicionar, remover ou alterar qualquer rota.
