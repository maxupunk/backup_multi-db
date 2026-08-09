# Roadmap — Paridade `backend` (AdonisJS) → `back-roco` (Rust/Loco)

> **Objetivo duplo**
> 1. Criar a suíte completa de **testes de endpoint** do `backend` atual, escrita de forma
>    **agnóstica de implementação**, para servir de *contrato executável* do `back-roco`.
> 2. Portar o `backend` para `back-roco` até a **paridade total**: 85 endpoints HTTP,
>    8 models, 14 migrations, 47 services, 4 middlewares, scheduler e SSE.
>
> Documento vivo. Marque os checkboxes conforme avança.
> Estimativas são ordens de grandeza, não compromissos.

---

## Sumário

- [1. Situação atual](#1-situação-atual)
- [2. Estratégia](#2-estratégia)
- [3. Decisões de arquitetura (bloqueantes)](#3-decisões-de-arquitetura-bloqueantes)
- [4. Inventário completo](#4-inventário-completo)
- [Fase 0 — Inventário e decisões](#fase-0--inventário-e-decisões)
- [Fase 1 — Harness de contrato](#fase-1--harness-de-contrato)
- [Fase 2 — Suíte de testes de endpoint](#fase-2--suíte-de-testes-de-endpoint)
- [Fase 3 — Fundação do back-roco](#fase-3--fundação-do-back-roco)
- [Fase 4 — Schema, migrations e entidades](#fase-4--schema-migrations-e-entidades)
- [Fase 5 — Auth, Users, Audit, System básico](#fase-5--auth-users-audit-system-básico)
- [Fase 6 — Connections + drivers de banco](#fase-6--connections--drivers-de-banco)
- [Fase 7 — Backups, dump e restore](#fase-7--backups-dump-e-restore)
- [Fase 8 — Storages (multi-provider)](#fase-8--storages-multi-provider)
- [Fase 9 — Docker Manager e Diagnostics](#fase-9--docker-manager-e-diagnostics)
- [Fase 10 — SSE, scheduler e workers](#fase-10--sse-scheduler-e-workers)
- [Fase 11 — System avançado e retenção](#fase-11--system-avançado-e-retenção)
- [Fase 12 — Paridade final e cutover](#fase-12--paridade-final-e-cutover)
- [Apêndice A — Matriz completa de endpoints](#apêndice-a--matriz-completa-de-endpoints)
- [Apêndice B — Mapa de dependências Node → Rust](#apêndice-b--mapa-de-dependências-node--rust)

---

## 1. Situação atual

### `backend/` — AdonisJS 7 (origem)

| Área | Quantidade |
|---|---|
| Endpoints HTTP | **85** (+ SSE `/__transmit/*` + fallback SPA) |
| Controllers | 10 (~3.138 LOC) |
| Models | 8 |
| Migrations | 14 |
| Services | 47 arquivos + 14 em `services/storage/` (~12.540 LOC) |
| Validators | 8 |
| Middlewares | 4 |
| Testes existentes | 20 functional + 14 unit + 1 shell (~5.322 LOC) |

Banco de controle: **SQLite** (`app_data/`). Bancos gerenciados: MySQL, MariaDB, PostgreSQL.

### `back-roco/` — Rust + Loco 1.0 (destino)

Estado: **scaffold inicial**. Contém apenas:

- `src/models/users.rs` + `_entities/users.rs`
- `src/controllers/auth.rs` (register/login/verify/forgot/reset/magic-link/current)
- 1 migration (`m20220101_000001_users.rs`)
- `tests/requests/auth.rs`, `tests/models/users.rs`, snapshots `insta`
- Worker `downloader`, task `user_create`, mailer `auth`

**Cobertura de paridade hoje: ~1%** (só o `users` do scaffold, com schema diferente do backend).

---

## 2. Estratégia

### Princípio central: o teste é o contrato

A suíte de endpoints **não** é escrita em Japa (acoplada ao Adonis) nem em `tests/requests/*.rs`
(acoplada ao Loco). Ela é escrita como **suíte black-box HTTP** que aponta para um `BASE_URL`:

```
BASE_URL=http://localhost:3333  pnpm contract:test   # roda contra o backend AdonisJS
BASE_URL=http://localhost:5150  pnpm contract:test   # roda contra o back-roco
```

Isso dá três ganhos:

1. **Especificação viva** — o comportamento do Adonis vira golden files antes de qualquer linha de Rust.
2. **Critério de aceite objetivo** — cada fase do port termina quando o subconjunto correspondente passa.
3. **Detecção de regressão bidirecional** — se o Adonis mudar durante o port, o contrato acusa.

As suítes nativas continuam existindo e são complementares:
- `backend/tests/unit/*` — permanece, valida lógica interna do Adonis.
- `back-roco/tests/**` — testes de model/task/worker em Rust, com `insta` (exigidos pelo `AGENTS.md`).

### Ordem de execução

```
Fase 0 (decisões)
   │
   ├─► Fase 1 (harness) ──► Fase 2 (85 testes de endpoint)   ← track de TESTES
   │                              │
   └─► Fase 3 (fundação) ──► Fase 4 (schema) ──► Fases 5..11 ← track de PORT
                                                     │
                                                     └─► Fase 12 (cutover)
```

Fases 1–2 e 3–4 podem correr **em paralelo**. As fases 5–11 consomem os testes da Fase 2 como
critério de pronto e são ordenadas por dependência (5 → 6 → 7 → 8 são sequenciais; 9 e 10 são
independentes e podem entrar a qualquer momento após a Fase 3).

---

## 3. Decisões de arquitetura (bloqueantes)

**Resolver ANTES de escrever código de produção.** Cada uma muda o desenho do port.

| # | Decisão | Opções | Impacto | Status |
|---|---|---|---|---|
| D1 | **Formato do token de auth** | (a) Manter *opaque token* do Adonis (`auth_access_tokens`, hash em DB) · (b) Migrar para JWT do Loco | (b) invalida todas as sessões e exige mudança no frontend; (a) exige reimplementar o provider do Adonis em Rust | ⬜ |
| D2 | **Hash de senha** | Adonis usa **scrypt**; Loco usa **argon2** | Se mudar, usuários existentes não conseguem logar → precisa de rehash-on-login ou reset forçado | ⬜ |
| D3 | **Criptografia de credenciais** | `EncryptionService` = AES-256-GCM, formato `iv:authTag:data` em base64 | O Rust **precisa** ler os registros existentes de `connections.password_encrypted` e `storage_destinations.config_encrypted`. Byte-compatibilidade obrigatória, incluindo a derivação de chave | ⬜ |
| D4 | **Estratégia de banco** | (a) `back-roco` aponta para o **mesmo** SQLite do Adonis · (b) schema novo + script de migração de dados | (a) exige que as migrations Sea-ORM sejam *no-op* sobre o schema existente; (b) exige janela de downtime | ⬜ |
| D5 | **Convenção de nomes de coluna** | Adonis: `snake_case` no DB / `camelCase` no JSON. Loco/Sea-ORM: `snake_case` em ambos | Se o JSON mudar, **o frontend quebra**. Definir política de serialização (`#[serde(rename_all = "camelCase")]`) desde a primeira DTO | ⬜ |
| D6 | **Transporte SSE** | Adonis usa `@adonisjs/transmit` em `/__transmit/*` | Loco não traz SSE. Reimplementar com `axum::response::sse` mantendo o mesmo path e formato de evento, ou trocar por WebSocket (quebra o frontend) | ⬜ |
| D7 | **Rate limiting** | `@adonisjs/limiter` com 4 limiters (`global`, `auth`, `strict`, `backup`) e `keyBy: 'ip-email'` | Reimplementar como middleware Axum (`tower-governor` ou próprio). Os headers de resposta (`X-RateLimit-*`, `Retry-After`) fazem parte do contrato | ⬜ |
| D8 | **Cutover** | (a) Big-bang · (b) Reverse-proxy por rota (strangler fig) | (b) permite migrar domínio a domínio com o frontend inalterado — **recomendado** | ⬜ |
| D9 | **Erros HTTP** | Adonis retorna `{ message, errors[] }` do VineJS | Definir shape único e traduzir os erros do `validator` para ele. Contrato do frontend | ⬜ |
| D10 | **Swagger** | `adonis-autoswagger` gera `/api/swagger` e `/api/docs` | Porta com `utoipa` ou aceita perda temporária da doc | ⬜ |

> **Recomendação para D1/D2/D4/D8:** manter opaque token + scrypt + mesmo SQLite + strangler fig.
> Zero impacto no frontend e zero downtime; o custo é reimplementar dois algoritmos em Rust,
> o que é contido e testável isoladamente.

---

## 4. Inventário completo

### 4.1 Models (8)

| Model | Tabela | Campos-chave | Particularidades |
|---|---|---|---|
| `User` | `users` | `email` (unique), `password`, `is_active`, `is_admin` | scrypt via `withAuthFinder`; `accessTokens` com TTL `AUTH_ACCESS_TOKEN_EXPIRES_IN` (default 7d) |
| `Connection` | `connections` | `type` (mysql/mariadb/postgresql), `host`, `port`, `password_encrypted`, `schedule_frequency` | hook `@beforeSave` criptografa; `getDecryptedPassword()`; `options` JSON |
| `ConnectionDatabase` | `connection_databases` | `connection_id`, `database_name`, `enabled` | unique(`connection_id`,`database_name`); cascade delete |
| `Backup` | `backups` | `status`, `file_path`, `file_size`, `checksum`, `retention_type` (GFS), `trigger` | `connection_id` **nullable** (migration 7); métodos `markAsStarted/Completed/Failed`, `promoteRetention` |
| `StorageDestination` | `storage_destinations` | `type` (local/s3/gcs/azure_blob/sftp), `provider`, `is_default`, `config_encrypted` | config criptografada + **cache WeakMap** por instância; `getSafeConfig()` mascara segredos |
| `AuditLog` | `audit_logs` | `action`, `entity_type`, `entity_id`, `status`, `details` JSON | append-only; enums viraram TEXT na migration 10 |
| `SystemSetting` | `system_settings` | `name` (unique), `value` JSON | usado pela política de retenção |
| `ResourceMetricHistory` | `resource_metric_history` | `scope` (system/container), `cpu_usage_percent`, `memory_*_bytes`, `collected_at` | série temporal; índices otimizados na migration 9 |

### 4.2 Migrations (14, na ordem de execução do Adonis)

```
1_create_connections_table
1z_create_connection_databases_table
2_create_backups_table
3_create_audit_logs_table
4_create_storage_destinations_table
5_add_storage_destination_id_to_connections_and_backups
6_extend_storage_destinations                      (+ provider, backfill de dados)
7_make_backups_connection_id_nullable
8_create_resource_metric_history_table
9_optimize_resource_metric_history_indexes
10_relax_audit_logs_enums                          (rebuild de tabela: enum → text)
1766179618065_create_users_table
1766179618097_create_access_tokens_table
1767200000000_create_system_settings_table
```

> ⚠️ As migrations 6 e 10 fazem **transformação de dados**, não só DDL. O port precisa
> replicar o efeito final, não a mecânica.

### 4.3 Middlewares (4)

- `auth_middleware` — guard de token, aplicado ao grupo protegido
- `rate_limit_middleware` — 4 limiters distintos, alguns com `keyBy: 'ip-email'`
- `force_json_response_middleware` — força `Accept: application/json` na API
- `container_bindings_middleware` — DI de request scope (sem equivalente/necessidade no Loco)

### 4.4 Services por cluster de complexidade

| Cluster | Arquivos | LOC aprox. | Risco do port |
|---|---|---|---|
| **Docker** | `docker_manager_service`, `docker_container_monitoring_service`, `docker_engine_http_client`, `docker_environment_service`, `docker_container_discovery_service`, `docker_diagnostics_service` + 4 runners de diagnóstico | ~3.000 | 🔴 Alto — HTTP sobre unix socket / named pipe |
| **Backup/Restore** | `backup_service`, `restore_service`, `backup_import_service`, `backup_retention_planner`, `retention_service`, `backup_retention_policy_service` | ~2.700 | 🔴 Alto — pipeline de subprocesso + streaming |
| **Storage** | 14 arquivos em `services/storage/` + `storage_destination_service`, `storage_space_service` | ~2.500 | 🟡 Médio — 5 SDKs de cloud |
| **Métricas/Sistema** | `system_monitoring_service`, `resource_metrics_*`, `container_memory_probe`, `memory_watermark_service` | ~1.200 | 🟡 Médio |
| **Notificação/SSE** | `notification_service`, `sse_subscribers`, 4 `*_emitter` | ~1.100 | 🟡 Médio — depende de D6 |
| **Infra** | `encryption_service`, `audit_service`, `scheduler_service`, `sqlite_runtime_config`, etc. | ~1.500 | 🟢 Baixo |

---

## Fase 0 — Inventário e decisões

**Duração estimada:** 2–3 dias · **Bloqueia:** tudo

- [ ] 0.1 — Revisar e **decidir D1 a D10** (seção 3). Registrar cada decisão com justificativa neste arquivo.
- [ ] 0.2 — Congelar o `backend/` durante o port (feature freeze) ou definir processo de sincronização de mudanças.
- [ ] 0.3 — Rodar `node ace list:routes` e salvar a saída como `docs/routes-baseline.txt` (fonte de verdade das 85 rotas).
- [ ] 0.4 — Extrair o Swagger atual (`GET /api/swagger`) e salvar como `docs/openapi-baseline.json`.
- [ ] 0.5 — Levantar o schema real do SQLite em produção: `sqlite3 app_data/db.sqlite3 .schema > docs/schema-baseline.sql`. Comparar com as migrations (detectar drift).
- [ ] 0.6 — Validar disponibilidade das crates do Apêndice B (versões, licenças, maturidade).
- [ ] 0.7 — Definir ambiente de teste reproduzível: `docker-compose.test.yml` com MySQL, MariaDB, PostgreSQL, MinIO e um SFTP — necessário para as fases 6–8.

**Pronto quando:** todas as decisões D1–D10 marcadas e o compose de teste sobe com um comando.

---

## Fase 1 — Harness de contrato

**Duração estimada:** 3–5 dias · **Depende de:** Fase 0

Cria a infraestrutura da suíte black-box. Nenhum teste de endpoint ainda — só o esqueleto.

- [ ] 1.1 — Criar `contract-tests/` na raiz do repositório (workspace independente, Node + Vitest + `undici`).
- [ ] 1.2 — Config de target: `BASE_URL`, `TARGET` (`adonis` | `roco`), timeouts, retries.
- [ ] 1.3 — Cliente HTTP com helpers: `as(user)`, `unauth()`, `expectStatus()`, `expectShape()`.
- [ ] 1.4 — **Gerência de estado determinístico** — decidir e implementar um dos:
  - reset do banco entre suítes via CLI de cada backend (`node ace migration:fresh --seed` / `cargo loco db reset`);
  - endpoint `POST /api/__test__/reset` habilitado só em `NODE_ENV=test`/`LOCO_ENV=test`;
  - fixtures idempotentes com prefixo de nome único por execução.
- [ ] 1.5 — Seeds compartilhados: usuário admin, usuário comum, usuário inativo, 1 conexão MySQL, 1 conexão PG, 1 storage local, 1 storage S3 (MinIO), backups em cada status.
- [ ] 1.6 — **Golden files**: modo `--record` que grava a resposta do Adonis em `contract-tests/__golden__/<endpoint>.json`, com redaction de campos voláteis (ids, timestamps, durations, paths temporários).
- [ ] 1.7 — Matchers tolerantes: comparação de *shape* + tipos + campos obrigatórios, não igualdade literal (evita falso-negativo por ordem de chaves ou id incremental).
- [ ] 1.8 — Relatório de cobertura de rotas: cruzar `routes-baseline.txt` × testes existentes e falhar o build se alguma rota ficar sem teste.
- [ ] 1.9 — Scripts: `pnpm contract:record`, `pnpm contract:adonis`, `pnpm contract:roco`, `pnpm contract:diff`.
- [ ] 1.10 — CI: job que roda a suíte contra o Adonis a cada PR (garante que os golden files não apodreçam).

**Pronto quando:** `pnpm contract:record` grava golden de `GET /api/health` e `pnpm contract:adonis` passa.

---

## Fase 2 — Suíte de testes de endpoint

**Duração estimada:** 3–4 semanas · **Depende de:** Fase 1 · **Paralelizável por lote**

85 endpoints. Para **cada** endpoint, cobrir no mínimo:

| Caso | Obrigatório |
|---|---|
| Caminho feliz (200/201/204) | ✅ |
| Sem autenticação → 401 | ✅ (rotas protegidas) |
| Payload inválido → 422 + shape de erro | ✅ (rotas com body) |
| Recurso inexistente → 404 | ✅ (rotas com `:id`) |
| Autorização insuficiente → 403 | ✅ (rotas admin-only) |
| Rate limit estourado → 429 + headers | ✅ (rotas com limiter) |
| Paginação / filtros / ordenação | ✅ (rotas de listagem) |

### Lote 2.1 — Público e Auth (6 endpoints)
- [ ] `GET /api/health` · `GET /api/swagger` · `GET /api/docs`
- [ ] `GET /api/auth/status` (estado de setup inicial)
- [ ] `POST /api/auth/register` — sucesso, e-mail duplicado, senha fraca, rate limit `auth`
- [ ] `POST /api/auth/login` — sucesso, senha errada, usuário inativo, rate limit `ip-email`
- [ ] `GET /api/auth/me` · `POST /api/auth/logout` — token válido, expirado, revogado, malformado

### Lote 2.2 — Users e Audit Logs (5 endpoints)
- [ ] `GET /api/users` — paginação, filtro, admin-only
- [ ] `PATCH /api/users/:id/status` — toggle, auto-desativação bloqueada, não-admin → 403
- [ ] `GET /api/audit-logs` — filtros por `action`, `entity_type`, `status`, range de datas, paginação
- [ ] `GET /api/audit-logs/stats` · `GET /api/audit-logs/:id`
- [ ] Verificar **efeito colateral**: ações em outros endpoints geram o `AuditLog` correto

### Lote 2.3 — Connections (10 endpoints)
- [ ] `GET /api/connections` — paginação, filtro por type/status, eager load de `databases`
- [ ] `POST /api/connections` — cada `type`, porta default, senha nunca serializada na resposta
- [ ] `GET /api/connections/:id` · `PUT /api/connections/:id` (incl. troca de senha) · `DELETE /api/connections/:id`
- [ ] `POST /api/connections/:id/test` — sucesso, host inválido, credencial errada, timeout, rate limit `strict`
- [ ] `POST /api/connections/:id/create-database` — sucesso, nome duplicado, nome inválido
- [ ] `POST /api/connections/:id/backup` — dispara backup, rate limit `backup`
- [ ] `POST /api/connections/discover-databases` — MySQL, MariaDB, PostgreSQL
- [ ] `GET /api/connections/docker-hosts` — com e sem Docker disponível
- [ ] `GET /api/connections/:connectionId/backups`

### Lote 2.4 — Backups (6 endpoints)
- [ ] `GET /api/backups` — filtros por status/connection/database, paginação, ordenação
- [ ] `GET /api/backups/:id` · `DELETE /api/backups/:id` (incl. `protected` → 409/422)
- [ ] `GET /api/backups/:id/download` — headers, `Content-Disposition`, arquivo ausente, storage remoto
- [ ] `POST /api/backups/:id/restore` — validação de filtros, target inválido, rate limit `strict`
- [ ] `POST /api/backups/import` — **multipart**: arquivo válido, extensão inválida, arquivo grande, rate limit `backup`

### Lote 2.5 — Storages + Storage Destinations (20 endpoints)
- [ ] CRUD `/api/storage-destinations` (5) + `/api/storage-destinations-space` + `/:id/space`
- [ ] CRUD `/api/storages` (5) — para **cada provider**: local, s3, gcs, azure_blob, sftp
- [ ] `POST /api/storages/:id/test` — sucesso e falha por provider
- [ ] `GET /api/storages/:id/browse` — paginação por prefixo, path traversal bloqueado
- [ ] `DELETE /api/storages/:id/object` — objeto único, inexistente, path traversal
- [ ] `POST /api/storages/:id/copy` + `GET /api/storages/copy-jobs/:jobId` — ciclo assíncrono completo
- [ ] `POST /api/storages/:id/archive` + `GET /api/storages/archive-jobs/:jobId` + `/download`
- [ ] Verificar **mascaramento de segredos** (`getSafeConfig`) em toda resposta

### Lote 2.6 — System (10 endpoints)
- [ ] `GET /api/stats` · `GET /api/system/status`
- [ ] `GET /api/system/diagnostics` · `/:name/download` · `DELETE /:name` — admin-only, path traversal
- [ ] `GET /api/system/containers/resources` · `GET /api/system/resources/history` (ranges e agregações)
- [ ] `GET|PUT /api/system/backup-retention` — validação da política GFS
- [ ] `POST /api/system/backup-retention/run` — dry-run e execução real

### Lote 2.7 — Docker Manager (25 endpoints)
- [ ] `GET /api/docker/status` — Docker disponível e indisponível
- [ ] Containers (9): list, inspect, logs (filtros `tail`/`since`/`stdout`/`stderr`), clear logs, start, stop, restart, remove
- [ ] Volumes (5): list, inspect, export, backup para storage, remove (com e sem `force`)
- [ ] Networks (5): list, inspect, create, connect, disconnect
- [ ] Images (4): list, inspect, remove, prune
- [ ] Diagnostics (2): `POST /api/docker/diagnostics` + `GET /:jobId` (ping, curl, port-scan)

### Lote 2.8 — SSE e não-HTTP
- [ ] `/__transmit/*` — subscribe, receber evento de progresso de backup, de restore, de recursos, de diagnóstico
- [ ] Fallback SPA `GET *` — com e sem `public/index.html`
- [ ] Headers globais: CORS, `force_json_response`, headers de rate limit

**Pronto quando:** relatório da tarefa 1.8 mostra **100% das 85 rotas cobertas** e a suíte
passa verde contra o Adonis.

---

## Fase 3 — Fundação do back-roco

**Duração estimada:** 1–2 semanas · **Depende de:** Fase 0 · **Paralela às Fases 1–2**

- [ ] 3.1 — Configuração `config/*.yaml` equivalente ao `.env` do Adonis (portas, DB, chave de cripto, TTL de token, limites). Segredos via `get_env`.
- [ ] 3.2 — **`EncryptionService` em Rust** (crate `aes-gcm`) — byte-compatível com o formato `iv:authTag:data` base64. **Teste crítico:** descriptografar um payload gerado pelo Node.
- [ ] 3.3 — **Hash de senha** conforme D2 (`scrypt` ou `argon2`) + verificação contra hashes existentes.
- [ ] 3.4 — **Auth** conforme D1: se opaque token, implementar o provider (`auth_access_tokens`, hash do token, `expires_at`, `last_used_at`) e o extractor Axum correspondente.
- [ ] 3.5 — **Middleware de rate limit** — 4 limiters, `keyBy` por IP e por IP+email, headers `X-RateLimit-*` e `Retry-After` idênticos.
- [ ] 3.6 — **Formato de erro unificado** (D9) — `impl IntoResponse` traduzindo erros do `validator` e do `loco_rs` para o shape do VineJS.
- [ ] 3.7 — Middleware equivalente ao `force_json_response` + CORS com a mesma config.
- [ ] 3.8 — **`AuditService`** em Rust — mesma assinatura de ações e enums; usado por todos os domínios seguintes.
- [ ] 3.9 — Estrutura de `src/dtos/` com política de serialização definida em D5 e export `ts-rs` para o frontend.
- [ ] 3.10 — Fixtures YAML em `src/fixtures/` espelhando os seeds da tarefa 1.5.
- [ ] 3.11 — `Dockerfile` e entrada no `docker-compose.dev.yml` para o `back-roco`.

**Pronto quando:** `cargo test` verde, `GET /api/health` responde idêntico ao Adonis no contrato,
e um token emitido pelo Adonis é aceito pelo Rust (se D1 = opaque).

---

## Fase 4 — Schema, migrations e entidades

**Duração estimada:** 1 semana · **Depende de:** Fase 3

- [ ] 4.1 — Escrever as migrations Sea-ORM para as 8 tabelas + `auth_access_tokens`, no formato `mYYYYMMDD_HHMMSS_<assunto>.rs`, registradas em `migration/src/lib.rs`.
- [ ] 4.2 — Reescrever a migration de `users` do scaffold para bater com o schema do Adonis (`full_name`, `is_active`, `is_admin`) — ou criar migration de ajuste, se já aplicada.
- [ ] 4.3 — Replicar **todos** os índices nomeados (são ~25 e afetam performance real).
- [ ] 4.4 — Replicar as constraints: unique de `connection_databases`, FKs com `CASCADE`/`SET NULL`.
- [ ] 4.5 — Conferir os enums: `connections.type/status/schedule_frequency`, `backups.status/retention_type/trigger`, `audit_logs.status`. Lembrar que `audit_logs.action/entity_type` são **TEXT**, não enum (migration 10).
- [ ] 4.6 — `cargo loco db migrate` + `cargo loco db entities` — gerar `src/models/_entities/`.
- [ ] 4.7 — Lógica de domínio em `src/models/*.rs`: hooks de criptografia (`ActiveModelBehavior`), `getDecryptedPassword`, `getSafeConfig`, `markAsStarted/Completed/Failed`, `promoteRetention`, `getDefaultPort`, `getDumpCommand`, `getScheduleIntervalMs`.
- [ ] 4.8 — **Validação de schema cruzado**: script que compara `.schema` do SQLite gerado pelo Rust com `docs/schema-baseline.sql` (Fase 0.5). Diferenças precisam ser justificadas.
- [ ] 4.9 — Conforme D4: script de migração de dados ou prova de que as migrations Sea-ORM são no-op sobre o banco existente.
- [ ] 4.10 — Testes de model em `back-roco/tests/models/` para os 8 models (`insta` + `#[serial]`).

**Pronto quando:** `cargo loco db migrate` roda limpo, o diff de schema da 4.8 está vazio (ou justificado),
e todos os testes de model passam.

---

## Fase 5 — Auth, Users, Audit, System básico

**Duração estimada:** 1–2 semanas · **Depende de:** Fase 4 · **Cobre lotes 2.1, 2.2 e parte do 2.6**

- [ ] 5.1 — `controllers/auth.rs` reescrito: `status`, `register`, `login`, `me`, `logout` — **descartar** os endpoints do scaffold que o Adonis não tem (`verify`, `forgot`, `reset`, `magic-link`) ou marcá-los como extensão consciente.
- [ ] 5.2 — Regra de `is_active`: usuário inativo não autentica.
- [ ] 5.3 — `controllers/users.rs`: `index` (paginado, admin-only), `toggle_status`.
- [ ] 5.4 — `controllers/audit_logs.rs`: `index` com todos os filtros, `stats`, `show`.
- [ ] 5.5 — `controllers/system.rs` parcial: `stats`, `status`.
- [ ] 5.6 — Aplicar rate limiters nas rotas correspondentes.
- [ ] 5.7 — Registrar tudo em `src/app.rs`; validar com `cargo loco routes`.
- [ ] 5.8 — Testes Rust de request espelhando os lotes 2.1 e 2.2.

**Pronto quando:** `BASE_URL=<roco> pnpm contract:test --grep "auth|users|audit"` passa 100%.

---

## Fase 6 — Connections + drivers de banco

**Duração estimada:** 2–3 semanas · **Depende de:** Fase 5 · **Cobre lote 2.3**

- [ ] 6.1 — CRUD de `connections` (`index`/`store`/`show`/`update`/`destroy`) + gestão de `connection_databases`.
- [ ] 6.2 — Validação equivalente ao `connection_validator.ts` via crate `validator`.
- [ ] 6.3 — **Drivers**: `sqlx` (MySQL + Postgres) ou `mysql_async` + `tokio-postgres`. Conexão com timeout, TLS conforme `options.ssl`.
- [ ] 6.4 — `POST /:id/test` — porta `performConnectionTest`, incluindo os modos de falha e o `last_error`/`last_tested_at`.
- [ ] 6.5 — `POST /discover-databases` — `SHOW DATABASES` / `pg_database`, com os mesmos filtros de bancos de sistema.
- [ ] 6.6 — `POST /:id/create-database` — DDL parametrizado, validação de nome.
- [ ] 6.7 — `GET /docker-hosts` — depende do cliente Docker da Fase 9; **stub aceitável aqui**, retornando lista vazia quando o Docker não está disponível.
- [ ] 6.8 — Porta de `connection_port_selection_resolver`, `connection_suggestion_mapper`, `container_port_resolver`, `network_reachability_resolver`.
- [ ] 6.9 — Auditoria: `connection.created/updated/deleted/tested`.
- [ ] 6.10 — Testes Rust de request + model.

**Pronto quando:** lote 2.3 do contrato passa contra o `back-roco` com MySQL, MariaDB e PG reais (compose da tarefa 0.7).

---

## Fase 7 — Backups, dump e restore

**Duração estimada:** 3–4 semanas · **Depende de:** Fase 6 · **Cobre lote 2.4** · 🔴 **Maior risco**

- [ ] 7.1 — `controllers/backups.rs`: `index`, `by_connection`, `show`, `destroy`.
- [ ] 7.2 — **Pipeline de dump** (`backup_service`): `tokio::process::Command` para `mysqldump`/`pg_dump`, streaming stdout → gzip → destino, sem bufferizar em memória.
- [ ] 7.3 — Cálculo de checksum SHA-256 em streaming, junto do gzip.
- [ ] 7.4 — Captura de `exit_code` e `stderr` com buffer limitado (porta de `process_output_buffer` e `child_process_exit`).
- [ ] 7.5 — `GET /:id/download` — streaming do arquivo, local e remoto, `Content-Disposition` idêntico.
- [ ] 7.6 — **Restore** (`restore_service`, 815 LOC): parsing de filtros, seleção de tabelas, pipeline de restauração. Portar junto os testes unitários `restore_filters*`, `restore_pipeline`.
- [ ] 7.7 — **Import** (`backup_import_service`): upload multipart, validação de formato, detecção de tipo de dump, armazenamento.
- [ ] 7.8 — Emissores de progresso (`backup_progress_emitter`, `restore_progress_emitter`) — dependem da Fase 10; usar canal interno agora, plugar no SSE depois.
- [ ] 7.9 — `backup_service_remote_cleanup` — remoção do arquivo no storage remoto ao deletar o backup.
- [ ] 7.10 — Regra de `protected` bloqueando delete e pruning.
- [ ] 7.11 — Auditoria: `backup.started/completed/failed/deleted/downloaded/imported`.
- [ ] 7.12 — Testes Rust: unit de pipeline + request.

**Pronto quando:** um backup real de MySQL e de PG é gerado, baixado, restaurado e validado
byte-a-byte contra o resultado do Adonis para o mesmo banco de origem.

---

## Fase 8 — Storages (multi-provider)

**Duração estimada:** 3–4 semanas · **Depende de:** Fase 7 · **Cobre lote 2.5**

- [ ] 8.1 — Trait `StorageExplorerAdapter` em Rust (espelho de `storage_explorer_adapter.ts`).
- [ ] 8.2 — Adapter **local** — incluindo bloqueio de path traversal.
- [ ] 8.3 — Adapter **S3** (`aws-sdk-s3`) — cobre AWS, MinIO e Cloudflare R2 (`force_path_style`, `endpoint`).
- [ ] 8.4 — Adapter **GCS** — sem SDK oficial em Rust; avaliar `google-cloud-storage` (comunidade) ou REST direto.
- [ ] 8.5 — Adapter **Azure Blob** (`azure_storage_blobs`).
- [ ] 8.6 — Adapter **SFTP** (`russh-sftp` ou `ssh2`) — auth por senha, chave privada e passphrase.
- [ ] 8.7 — CRUD de `storages` + `storage-destinations` (rotas legadas mantidas).
- [ ] 8.8 — `POST /:id/test` por provider.
- [ ] 8.9 — `GET /:id/browse` — paginação por continuation token, mesma ordenação e shape.
- [ ] 8.10 — `DELETE /:id/object`.
- [ ] 8.11 — **Copy job** assíncrono (`bucket_copy_service`, 455 LOC) — job em background + endpoint de status.
- [ ] 8.12 — **Archive job** (`bucket_archive_service`, 395 LOC) — streaming de tar/zip, endpoint de status e download.
- [ ] 8.13 — `storage_space_service` — cálculo de espaço por destino e agregado.
- [ ] 8.14 — Retenção de jobs (porta de `storage_job_retention`).
- [ ] 8.15 — Memoização da config descriptografada — o equivalente ao `WeakMap` do TS (o custo que motivou o cache é real: 2 operações de cripto **por objeto listado**).
- [ ] 8.16 — Testes Rust + contrato contra MinIO e SFTP do compose.

**Pronto quando:** lote 2.5 passa para local, S3/MinIO e SFTP. GCS e Azure podem ficar
com teste de integração opcional se não houver credencial em CI — **registrar a lacuna**.

---

## Fase 9 — Docker Manager e Diagnostics

**Duração estimada:** 2–3 semanas · **Depende de:** Fase 3 · **Cobre lote 2.7** · Independente das Fases 5–8

- [ ] 9.1 — Cliente Docker: `bollard` (recomendado) ou porta manual de `docker_engine_http_client` sobre unix socket / named pipe.
- [ ] 9.2 — `docker_environment_service` — detecção de ambiente (dentro/fora de container, socket disponível).
- [ ] 9.3 — `GET /api/docker/status`.
- [ ] 9.4 — **Containers** (9 endpoints): list, inspect, logs com filtros, clear logs, start, stop, restart, remove.
- [ ] 9.5 — **Volumes** (5): list, inspect, export (streaming tar), backup para storage, remove.
- [ ] 9.6 — **Networks** (5): list, inspect, create, connect, disconnect.
- [ ] 9.7 — **Images** (4): list, inspect, remove, prune.
- [ ] 9.8 — **Diagnostics** (2): job assíncrono + os 3 runners (ping, curl, port-scan).
- [ ] 9.9 — `docker_container_discovery_service` — alimenta `GET /api/connections/docker-hosts` (fecha a pendência 6.7).
- [ ] 9.10 — `docker_container_monitoring_service` (757 LOC) — coleta de stats, alimenta a Fase 11.
- [ ] 9.11 — `container_memory_probe` — leitura de cgroup v1/v2.
- [ ] 9.12 — Testes Rust + contrato (usar containers descartáveis criados pelo próprio teste).

**Pronto quando:** lote 2.7 passa com Docker disponível **e** com Docker indisponível (degradação idêntica).

---

## Fase 10 — SSE, scheduler e workers

**Duração estimada:** 2 semanas · **Depende de:** Fase 3 · **Cobre lote 2.8**

- [ ] 10.1 — Conforme D6: endpoint SSE em `/__transmit/*` com `axum::response::sse`, replicando o handshake e o formato de evento do `@adonisjs/transmit`.
- [ ] 10.2 — Registry de subscribers (porta de `sse_subscribers`) com broadcast por canal.
- [ ] 10.3 — Plugar os 4 emissores: backup progress, restore progress, resource metrics, docker diagnostics.
- [ ] 10.4 — `notification_service` (526 LOC).
- [ ] 10.5 — **Scheduler** — `scheduler_service` (node-cron) → scheduler do Loco (`config/scheduler.yaml`). Backups agendados por `schedule_frequency` (1h/6h/12h/24h) e `schedule_enabled`.
- [ ] 10.6 — Sincronização do scheduler no CRUD de connections (equivalente ao `syncScheduler`).
- [ ] 10.7 — Workers em background para copy/archive jobs (Fase 8) e coleta de métricas (Fase 11).
- [ ] 10.8 — Retenção automática de audit logs (`audit_retention_service`).
- [ ] 10.9 — Testes de worker e task em `back-roco/tests/workers/` e `tests/tasks/`.

**Pronto quando:** um backup manual disparado emite os mesmos eventos SSE que o Adonis, e o
frontend atual consome o stream do `back-roco` sem alteração.

---

## Fase 11 — System avançado e retenção

**Duração estimada:** 1–2 semanas · **Depende de:** Fases 9 e 10 · **Fecha o lote 2.6**

- [ ] 11.1 — `GET /api/system/containers/resources`.
- [ ] 11.2 — `resource_metrics_polling_service` + `resource_metrics_history_service` — coleta periódica e persistência em `resource_metric_history`.
- [ ] 11.3 — `GET /api/system/resources/history` — ranges, agregação e downsampling idênticos.
- [ ] 11.4 — `GET|PUT /api/system/backup-retention` — política GFS em `system_settings`.
- [ ] 11.5 — `backup_retention_planner` — portar junto o teste unitário existente (a lógica de projeção é sutil).
- [ ] 11.6 — `POST /api/system/backup-retention/run`.
- [ ] 11.7 — **Diagnostics de sistema** (3 endpoints): listar, baixar e remover heap snapshots — admin-only, com bloqueio de path traversal. Avaliar se o conceito ainda faz sentido em Rust (heap snapshot é artefato do V8) — pode virar profile de memória ou ser descontinuado com registro da decisão.
- [ ] 11.8 — `memory_watermark_service` e instrumentação de memória — reavaliar necessidade.
- [ ] 11.9 — `system_monitoring_service` — métricas de host via `sysinfo`.

**Pronto quando:** lote 2.6 passa integralmente (ou com as exceções de 11.7/11.8 registradas e aceitas).

---

## Fase 12 — Paridade final e cutover

**Duração estimada:** 2–3 semanas · **Depende de:** todas as anteriores

- [ ] 12.1 — **Suíte de contrato 100% verde** contra o `back-roco`. Zero rota descoberta.
- [ ] 12.2 — Diff automatizado Adonis × Roco: rodar a suíte contra os dois em paralelo e comparar respostas campo a campo.
- [ ] 12.3 — Swagger conforme D10 — comparar com `openapi-baseline.json`.
- [ ] 12.4 — Fallback SPA + `@adonisjs/static` equivalente (servir `public/`).
- [ ] 12.5 — **Frontend contra o `back-roco`** — rodar a suíte E2E do frontend sem nenhuma alteração de código.
- [ ] 12.6 — **Benchmark** — latência p50/p95/p99 e uso de memória nos endpoints quentes (listagens, browse, métricas). Documentar o ganho.
- [ ] 12.7 — Teste de carga nos jobs assíncronos (backup de banco grande, archive de bucket grande).
- [ ] 12.8 — Revisão de segurança do diff: sem segredo em log, sem `unwrap` em handler, criptografia validada, path traversal coberto em todos os pontos.
- [ ] 12.9 — `cargo fmt` + `cargo clippy --all-targets -- -D warnings` limpos.
- [ ] 12.10 — Dockerfile de produção multi-stage + entrada no `docker-compose.yml`.
- [ ] 12.11 — Documentação: README, AGENTS.md atualizado, guia de migração, runbook de rollback.
- [ ] 12.12 — **Cutover** conforme D8. Se strangler fig: proxy roteando domínio a domínio, com plano de rollback por rota.
- [ ] 12.13 — Período de shadow traffic (rodar os dois, comparar respostas em produção, servir só o Adonis) antes de trocar de fato.
- [ ] 12.14 — Descomissionar o `backend/` só após N dias de estabilidade.

---

## Resumo de esforço

| Fase | Escopo | Estimativa | Risco |
|---|---|---|---|
| 0 | Inventário e decisões | 2–3 dias | 🟢 |
| 1 | Harness de contrato | 3–5 dias | 🟢 |
| 2 | 85 testes de endpoint | 3–4 semanas | 🟡 |
| 3 | Fundação | 1–2 semanas | 🟡 |
| 4 | Schema e migrations | 1 semana | 🟡 |
| 5 | Auth/Users/Audit/System | 1–2 semanas | 🟢 |
| 6 | Connections + drivers | 2–3 semanas | 🟡 |
| 7 | Backups/dump/restore | 3–4 semanas | 🔴 |
| 8 | Storages multi-provider | 3–4 semanas | 🔴 |
| 9 | Docker Manager | 2–3 semanas | 🔴 |
| 10 | SSE/scheduler/workers | 2 semanas | 🟡 |
| 11 | System avançado | 1–2 semanas | 🟡 |
| 12 | Paridade e cutover | 2–3 semanas | 🟡 |

**Total sequencial: ~5–7 meses.** Com as Fases 1–2 em paralelo às 3–4, e a Fase 9 em paralelo
às 6–8, cai para **~4–5 meses** com duas frentes de trabalho.

---

## Apêndice A — Matriz completa de endpoints

Legenda: **T** = teste de contrato escrito (Fase 2) · **P** = portado no `back-roco` · **V** = verde contra o Roco

### Público

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 1 | GET | `/api/health` | inline | global | ⬜ | ⬜ | ⬜ |
| 2 | GET | `/api/swagger` | autoswagger | global | ⬜ | ⬜ | ⬜ |
| 3 | GET | `/api/docs` | autoswagger | global | ⬜ | ⬜ | ⬜ |
| 4 | GET | `/api/auth/status` | Auth.checkStatus | global | ⬜ | ⬜ | ⬜ |
| 5 | POST | `/api/auth/register` | Auth.register | auth (ip-email) | ⬜ | ⬜ | ⬜ |
| 6 | POST | `/api/auth/login` | Auth.login | auth (ip-email) | ⬜ | ⬜ | ⬜ |

### Auth protegido

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 7 | GET | `/api/auth/me` | Auth.me | global | ⬜ | ⬜ | ⬜ |
| 8 | POST | `/api/auth/logout` | Auth.logout | global | ⬜ | ⬜ | ⬜ |

### Connections

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 9 | POST | `/api/connections/discover-databases` | Connections.discoverDatabases | strict | ⬜ | ⬜ | ⬜ |
| 10 | GET | `/api/connections/docker-hosts` | Connections.dockerHosts | global | ⬜ | ⬜ | ⬜ |
| 11 | GET | `/api/connections` | Connections.index | global | ⬜ | ⬜ | ⬜ |
| 12 | POST | `/api/connections` | Connections.store | global | ⬜ | ⬜ | ⬜ |
| 13 | GET | `/api/connections/:id` | Connections.show | global | ⬜ | ⬜ | ⬜ |
| 14 | PUT/PATCH | `/api/connections/:id` | Connections.update | global | ⬜ | ⬜ | ⬜ |
| 15 | DELETE | `/api/connections/:id` | Connections.destroy | global | ⬜ | ⬜ | ⬜ |
| 16 | POST | `/api/connections/:id/test` | Connections.test | strict | ⬜ | ⬜ | ⬜ |
| 17 | POST | `/api/connections/:id/create-database` | Connections.createDatabase | strict | ⬜ | ⬜ | ⬜ |
| 18 | POST | `/api/connections/:id/backup` | Connections.backup | backup | ⬜ | ⬜ | ⬜ |

### Storage Destinations (legado)

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 19 | GET | `/api/storage-destinations` | StorageDestinations.index | global | ⬜ | ⬜ | ⬜ |
| 20 | POST | `/api/storage-destinations` | StorageDestinations.store | global | ⬜ | ⬜ | ⬜ |
| 21 | GET | `/api/storage-destinations/:id` | StorageDestinations.show | global | ⬜ | ⬜ | ⬜ |
| 22 | PUT/PATCH | `/api/storage-destinations/:id` | StorageDestinations.update | global | ⬜ | ⬜ | ⬜ |
| 23 | DELETE | `/api/storage-destinations/:id` | StorageDestinations.destroy | global | ⬜ | ⬜ | ⬜ |
| 24 | GET | `/api/storage-destinations-space` | StorageDestinations.spaceAll | global | ⬜ | ⬜ | ⬜ |
| 25 | GET | `/api/storage-destinations/:id/space` | StorageDestinations.space | global | ⬜ | ⬜ | ⬜ |

### Storages

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 26 | GET | `/api/storages` | Storages.index | global | ⬜ | ⬜ | ⬜ |
| 27 | POST | `/api/storages` | Storages.store | global | ⬜ | ⬜ | ⬜ |
| 28 | GET | `/api/storages/copy-jobs/:jobId` | Storages.copyStatus | global | ⬜ | ⬜ | ⬜ |
| 29 | GET | `/api/storages/archive-jobs/:jobId` | Storages.archiveJobStatus | global | ⬜ | ⬜ | ⬜ |
| 30 | GET | `/api/storages/archive-jobs/:jobId/download` | Storages.downloadArchive | global | ⬜ | ⬜ | ⬜ |
| 31 | GET | `/api/storages/:id` | Storages.show | global | ⬜ | ⬜ | ⬜ |
| 32 | PUT | `/api/storages/:id` | Storages.update | global | ⬜ | ⬜ | ⬜ |
| 33 | DELETE | `/api/storages/:id` | Storages.destroy | global | ⬜ | ⬜ | ⬜ |
| 34 | POST | `/api/storages/:id/test` | Storages.test | strict | ⬜ | ⬜ | ⬜ |
| 35 | GET | `/api/storages/:id/browse` | Storages.browse | global | ⬜ | ⬜ | ⬜ |
| 36 | DELETE | `/api/storages/:id/object` | Storages.destroyObject | global | ⬜ | ⬜ | ⬜ |
| 37 | POST | `/api/storages/:id/copy` | Storages.startCopy | backup | ⬜ | ⬜ | ⬜ |
| 38 | POST | `/api/storages/:id/archive` | Storages.startArchive | backup | ⬜ | ⬜ | ⬜ |

### Backups

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 39 | GET | `/api/backups` | Backups.index | global | ⬜ | ⬜ | ⬜ |
| 40 | GET | `/api/connections/:connectionId/backups` | Backups.byConnection | global | ⬜ | ⬜ | ⬜ |
| 41 | GET | `/api/backups/:id` | Backups.show | global | ⬜ | ⬜ | ⬜ |
| 42 | GET | `/api/backups/:id/download` | Backups.download | global | ⬜ | ⬜ | ⬜ |
| 43 | POST | `/api/backups/:id/restore` | Backups.restore | strict | ⬜ | ⬜ | ⬜ |
| 44 | DELETE | `/api/backups/:id` | Backups.destroy | global | ⬜ | ⬜ | ⬜ |
| 45 | POST | `/api/backups/import` | Backups.import | backup | ⬜ | ⬜ | ⬜ |

### System

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 46 | GET | `/api/stats` | System.stats | global | ⬜ | ⬜ | ⬜ |
| 47 | GET | `/api/system/status` | System.status | global | ⬜ | ⬜ | ⬜ |
| 48 | GET | `/api/system/diagnostics` | System.diagnostics | global | ⬜ | ⬜ | ⬜ |
| 49 | GET | `/api/system/diagnostics/:name/download` | System.downloadDiagnostic | strict | ⬜ | ⬜ | ⬜ |
| 50 | DELETE | `/api/system/diagnostics/:name` | System.destroyDiagnostic | strict | ⬜ | ⬜ | ⬜ |
| 51 | GET | `/api/system/containers/resources` | System.containerResources | global | ⬜ | ⬜ | ⬜ |
| 52 | GET | `/api/system/resources/history` | System.resourcesHistory | global | ⬜ | ⬜ | ⬜ |
| 53 | GET | `/api/system/backup-retention` | System.backupRetentionPolicy | global | ⬜ | ⬜ | ⬜ |
| 54 | PUT | `/api/system/backup-retention` | System.updateBackupRetentionPolicy | strict | ⬜ | ⬜ | ⬜ |
| 55 | POST | `/api/system/backup-retention/run` | System.runBackupRetention | strict | ⬜ | ⬜ | ⬜ |

### Audit Logs

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 56 | GET | `/api/audit-logs` | AuditLogs.index | global | ⬜ | ⬜ | ⬜ |
| 57 | GET | `/api/audit-logs/stats` | AuditLogs.stats | global | ⬜ | ⬜ | ⬜ |
| 58 | GET | `/api/audit-logs/:id` | AuditLogs.show | global | ⬜ | ⬜ | ⬜ |

### Users

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 59 | GET | `/api/users` | Users.index | global | ⬜ | ⬜ | ⬜ |
| 60 | PATCH | `/api/users/:id/status` | Users.toggleStatus | global | ⬜ | ⬜ | ⬜ |

### Docker — Containers

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 61 | GET | `/api/docker/status` | DockerManager.status | global | ⬜ | ⬜ | ⬜ |
| 62 | GET | `/api/docker/containers` | DockerManager.listContainers | global | ⬜ | ⬜ | ⬜ |
| 63 | GET | `/api/docker/containers/:id` | DockerManager.inspectContainer | global | ⬜ | ⬜ | ⬜ |
| 64 | GET | `/api/docker/containers/:id/logs` | DockerManager.containerLogs | global | ⬜ | ⬜ | ⬜ |
| 65 | DELETE | `/api/docker/containers/:id/logs` | DockerManager.clearContainerLogs | strict | ⬜ | ⬜ | ⬜ |
| 66 | POST | `/api/docker/containers/:id/start` | DockerManager.startContainer | strict | ⬜ | ⬜ | ⬜ |
| 67 | POST | `/api/docker/containers/:id/stop` | DockerManager.stopContainer | strict | ⬜ | ⬜ | ⬜ |
| 68 | POST | `/api/docker/containers/:id/restart` | DockerManager.restartContainer | strict | ⬜ | ⬜ | ⬜ |
| 69 | DELETE | `/api/docker/containers/:id` | DockerManager.removeContainer | strict | ⬜ | ⬜ | ⬜ |

### Docker — Volumes

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 70 | GET | `/api/docker/volumes` | DockerManager.listVolumes | global | ⬜ | ⬜ | ⬜ |
| 71 | GET | `/api/docker/volumes/:name` | DockerManager.inspectVolume | global | ⬜ | ⬜ | ⬜ |
| 72 | GET | `/api/docker/volumes/:name/export` | DockerManager.exportVolume | strict | ⬜ | ⬜ | ⬜ |
| 73 | POST | `/api/docker/volumes/:name/backup` | DockerManager.backupVolumeToStorage | backup | ⬜ | ⬜ | ⬜ |
| 74 | DELETE | `/api/docker/volumes/:name` | DockerManager.removeVolume | strict | ⬜ | ⬜ | ⬜ |

### Docker — Networks

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 75 | GET | `/api/docker/networks` | DockerManager.listNetworks | global | ⬜ | ⬜ | ⬜ |
| 76 | GET | `/api/docker/networks/:id` | DockerManager.inspectNetwork | global | ⬜ | ⬜ | ⬜ |
| 77 | POST | `/api/docker/networks` | DockerManager.createNetwork | strict | ⬜ | ⬜ | ⬜ |
| 78 | POST | `/api/docker/networks/:id/connect` | DockerManager.connectContainerToNetwork | strict | ⬜ | ⬜ | ⬜ |
| 79 | POST | `/api/docker/networks/:id/disconnect` | DockerManager.disconnectContainerFromNetwork | strict | ⬜ | ⬜ | ⬜ |

### Docker — Diagnostics e Images

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 80 | POST | `/api/docker/diagnostics` | DockerDiagnostics.start | strict | ⬜ | ⬜ | ⬜ |
| 81 | GET | `/api/docker/diagnostics/:jobId` | DockerDiagnostics.show | global | ⬜ | ⬜ | ⬜ |
| 82 | POST | `/api/docker/images/prune` | DockerManager.pruneImages | strict | ⬜ | ⬜ | ⬜ |
| 83 | GET | `/api/docker/images` | DockerManager.listImages | global | ⬜ | ⬜ | ⬜ |
| 84 | GET | `/api/docker/images/:id` | DockerManager.inspectImage | global | ⬜ | ⬜ | ⬜ |
| 85 | DELETE | `/api/docker/images/:id` | DockerManager.removeImage | strict | ⬜ | ⬜ | ⬜ |

### Não-API

| # | Método | Rota | Origem | T | P | V |
|---|---|---|---|:-:|:-:|:-:|
| 86 | GET/POST | `/__transmit/*` | `@adonisjs/transmit` (SSE) | ⬜ | ⬜ | ⬜ |
| 87 | GET | `*` | Fallback SPA | ⬜ | ⬜ | ⬜ |

> ⚠️ **Ordem de rotas importa.** Várias rotas específicas precisam ser registradas **antes**
> das paramétricas: `connections/discover-databases` antes de `connections/:id`,
> `storages/copy-jobs/:jobId` antes de `storages/:id`, `backups/import` antes de `backups/:id`,
> `docker/images/prune` antes de `docker/images/:id`. O Axum resolve conflitos de forma
> diferente do roteador do Adonis — **testar explicitamente cada um desses pares**.

---

## Apêndice B — Mapa de dependências Node → Rust

| Node / Adonis | Uso | Candidato em Rust | Confiança |
|---|---|---|---|
| `@adonisjs/core` | framework | `loco-rs` + `axum` | ✅ direto |
| `@adonisjs/lucid` | ORM | `sea-orm` | ✅ direto |
| `@adonisjs/auth` | tokens de acesso | auth do Loco ou implementação própria (D1) | ⚠️ depende de D1 |
| `@adonisjs/limiter` | rate limit | `tower-governor` ou middleware próprio | ⚠️ headers precisam bater |
| `@adonisjs/transmit` | SSE | `axum::response::sse` | ⚠️ protocolo precisa bater (D6) |
| `@adonisjs/cors` | CORS | `tower-http::cors` | ✅ direto |
| `@adonisjs/static` | arquivos estáticos | `tower-http::services::ServeDir` | ✅ direto |
| `@vinejs/vine` | validação | `validator` | ✅ direto (shape de erro difere) |
| `node:crypto` AES-256-GCM | `EncryptionService` | `aes-gcm` + `base64` | ✅ direto — **exige teste de compatibilidade** |
| scrypt (hash) | senha de usuário | `scrypt` | ✅ direto |
| `mysql2` | driver MySQL/MariaDB | `sqlx` (mysql) ou `mysql_async` | ✅ direto |
| `pg` | driver PostgreSQL | `sqlx` (postgres) ou `tokio-postgres` | ✅ direto |
| `better-sqlite3` | SQLite de controle | `sqlx` (sqlite) via Sea-ORM | ✅ direto |
| `@aws-sdk/client-s3` | S3/MinIO/R2 | `aws-sdk-s3` | ✅ direto |
| `@aws-sdk/s3-request-presigner` | URLs assinadas | `aws-sdk-s3` (presigning) | ✅ direto |
| `@google-cloud/storage` | GCS | `google-cloud-storage` (comunidade) ou REST | 🔴 sem SDK oficial |
| `@azure/storage-blob` | Azure Blob | `azure_storage_blobs` | ⚠️ SDK em preview |
| `ssh2-sftp-client` | SFTP | `russh-sftp` ou `ssh2` | ⚠️ avaliar |
| `archiver` | tar/zip streaming | `tar` + `flate2` / `zip` | ✅ direto |
| `node-cron` | agendamento | scheduler do Loco | ✅ direto |
| `luxon` | datas | `chrono` | ✅ direto |
| `adonis-autoswagger` | OpenAPI | `utoipa` | ⚠️ geração manual (D10) |
| Docker HTTP client próprio | Docker Engine API | `bollard` | ✅ direto — cobre unix socket e named pipe |
| `child_process` | mysqldump/pg_dump | `tokio::process` | ✅ direto |
| — | métricas de host | `sysinfo` | ✅ direto |
| `@japa/*` | testes | contract suite (Fase 1) + `loco_rs::testing` | ✅ |

---

## Riscos e mitigações

| Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|
| Criptografia incompatível → credenciais existentes ilegíveis | Média | 🔴 Crítico | Tarefa 3.2 com teste cruzado Node→Rust **antes** de qualquer outra coisa |
| GCS sem SDK oficial em Rust | Alta | 🟡 Médio | Implementar via REST + OAuth2, ou manter GCS no Adonis durante o strangler fig |
| Comportamento sutil de restore diverge (filtros) | Alta | 🔴 Crítico | Portar os 3 testes unitários de `restore_filters*` primeiro, como spec |
| Ordem de rotas do Axum difere do Adonis | Alta | 🟡 Médio | Testes explícitos para os 4 pares conflitantes (nota do Apêndice A) |
| Shape do JSON muda e quebra o frontend | Média | 🔴 Crítico | D5 decidido cedo + golden files da Fase 1 + tarefa 12.5 |
| Backend continua evoluindo durante o port | Alta | 🟡 Médio | Feature freeze (0.2) ou CI que roda o contrato contra os dois |
| Escopo dos 47 services subestimado | Média | 🟡 Médio | Reavaliar estimativa ao fim da Fase 6, com dados reais de velocidade |

---

## Como usar este documento

1. Comece **exclusivamente** pela Fase 0. Nenhuma linha de código de produção antes das decisões D1–D10.
2. Marque os checkboxes conforme conclui. Um item só é marcado quando o critério de "Pronto quando" da fase o cobre.
3. Atualize o Apêndice A (colunas **T**/**P**/**V**) a cada endpoint concluído — é o placar real de paridade.
4. Se uma decisão mudar no meio do caminho, edite a seção 3 registrando **o motivo** — desvio silencioso vira dívida (regra da seção 12 do `AGENTS.md`).
