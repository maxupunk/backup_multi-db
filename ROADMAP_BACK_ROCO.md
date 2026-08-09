# Roadmap — Paridade `backend` (AdonisJS) → `back-roco` (Rust/Loco)

> **Objetivo duplo**
> 1. Criar a suíte completa de **testes de endpoint** do `backend` atual, escrita de forma
>    **agnóstica de implementação**, para servir de *contrato executável* do `back-roco`.
> 2. Portar o `backend` para `back-roco` até a **paridade total**: **87 pares método+rota**
>    sob `/api` (85 endpoints lógicos), 8 models, 14 migrations, 47 services, 4 middlewares,
>    3 rotas SSE, scheduler e fallback SPA.
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
| Endpoints HTTP | **87 pares método+rota** sob `/api` (85 handlers lógicos) + 3 rotas SSE + fallback SPA = **91** |
| Controllers | 10 (~3.138 LOC) |
| Models | 8 |
| Migrations | 14 |
| Services | 47 arquivos + 14 em `services/storage/` (~12.540 LOC) |
| Validators | 8 |
| Middlewares | 4 |
| Testes existentes | 20 functional + 14 unit + 1 shell (~5.322 LOC) |

Banco de controle: **SQLite** (`app_data/`). Bancos gerenciados: MySQL, MariaDB, PostgreSQL.

### `back-roco/` — Rust + Loco 1.0 (destino)

> Esta seção descrevia o **scaffold inicial**. Foi reescrita ao fim da Fase 7; o texto original
> está no git.

Estado: **Fases 0 a 7 concluídas.**

| Área | Estado |
|---|---|
| Endpoints sob `/api` | **30 de 87** pares método+rota (`cargo loco routes`) |
| Controllers | `auth`, `users`, `audit_logs`, `system` (parcial), `connections`, `backups` |
| Models | 9 entidades geradas + lógica de domínio, criptografia, senha, token, driver de banco, dump, restore, import, progresso |
| Migrations | 4, cobrindo as 9 tabelas · diff estrutural vazio contra o schema do Adonis |
| Workers | `downloader` (scaffold), `restore` |
| Testes | **457**, 0 falhas, 4 ignorados (2 de dados de produção, 2 de servidor real) |

**Cobertura de paridade hoje: 34%** dos pares método+rota. O que falta: storages (Fase 8), Docker
(Fase 9), SSE e scheduler (Fase 10), sistema avançado e retenção (Fase 11).

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

**Status: ✅ TODAS DECIDIDAS em 2026-08-09.** Alterações a partir daqui exigem registro do motivo.

| # | Decisão | **Escolha** | Consequência assumida |
|---|---|---|---|
| D1 | Formato do token de auth | ✅ **Token opaco compatível** — mantém `auth_access_tokens`, hash em DB | Sessões atuais continuam válidas; frontend inalterado. Custo: reimplementar o provider do Adonis em Rust |
| D2 | Hash de senha | ✅ **scrypt** (o mesmo do Adonis) | Usuários logam com a senha atual, sem reset nem rehash. Custo: `scrypt` crate + réplica dos parâmetros do `@adonisjs/core/hash` |
| D3 | Criptografia de credenciais | ✅ **AES-256-GCM byte-compatível**, formato `iv:authTag:data` base64 | ⚠️ **IV de 16 bytes**, não os 12 padrão do GCM — o Rust precisa de `AesGcm<Aes256, U16>`, não o alias `Aes256Gcm`. Chave = `DB_ENCRYPTION_KEY` hex de 64 chars, usada **direto**, sem KDF |
| D4 | Estratégia de banco | ✅ **Schema novo + script de migração de dados** | Liberdade para modelar no estilo Sea-ORM. Exige janela de downtime no cutover e um migrador que preserve os dados atuais (incl. 25.458 linhas de métricas) e o hash dos tokens |
| D5 | Nomes na serialização | ✅ **`camelCase` no JSON**, `snake_case` no DB | Todas as DTOs levam `#[serde(rename_all = "camelCase")]`. Frontend não muda |
| D6 | Transporte SSE | ✅ **`axum::response::sse` no mesmo path** `/__transmit/*` | Replicar o protocolo do `@adonisjs/transmit`: `GET /__transmit/events`, `POST /__transmit/subscribe`, `POST /__transmit/unsubscribe` |
| D7 | Rate limiting | ✅ **Middleware Axum próprio** (base `tower-governor`) | 4 limiters (`global`, `auth`, `strict`, `backup`), `keyBy` por IP e IP+email, headers `X-RateLimit-*` e `Retry-After` idênticos |
| D8 | Cutover | ✅ **Big-bang ao final da Fase 12** | Sem proxy intermediário. Todo o risco concentrado num evento → as Fases 12.12–12.14 (shadow traffic e runbook de rollback) passam a ser **obrigatórias**, não opcionais |
| D9 | Erros HTTP | ⚠️ **Corrigida na Fase 2** — são **duas** famílias, não uma. Ver abaixo | `impl IntoResponse` para a família do framework + helper para a dos controllers |
| D10 | Swagger | ✅ **`utoipa`**, comparado contra `docs/openapi-baseline.yml` | Anotação manual dos handlers; 73 paths a cobrir |

### D9 corrigida — a API tem duas famílias de erro, não uma

A decisão D9 foi tomada na Fase 0 assumindo um shape único. Os goldens gravados na Fase 2
mostram que isso está errado. Convivem **duas** famílias, e o back-roco precisa reproduzir as
duas:

| Origem | Shape | Onde aparece |
|---|---|---|
| Framework (VineJS, `E_INVALID_CREDENTIALS`, limiter) | `{ "errors": [ … ] }` — **sem** `success`, **sem** `message` no topo | 422 de validação, 400 de credencial inválida, 429 de rate limit |
| Escrita à mão nos controllers | `{ "success": false, "message": "…" }` — **sem** `errors` | 401 de conta pendente, 403 de não-admin, 404 de recurso, 400 de regra de negócio |

Exemplos reais, extraídos dos goldens:

```jsonc
// 422 — POST /api/auth/register com e-mail duplicado
{ "errors": [ { "field": "email", "message": "The email has already been taken", "rule": "database.unique" } ] }

// 400 — POST /api/auth/login com senha errada
{ "errors": [ { "message": "Invalid user credentials" } ] }

// 429 — limiter de `auth` estourado (headers: `retry-after: 60`, `x-ratelimit-limit: 5`)
{ "errors": [ { "message": "Too many requests", "retryAfter": 60 } ] }

// 403 — GET /api/users como não-admin
{ "success": false, "message": "Apenas administradores podem gerenciar usuarios." }
```

Repare que o item de `errors` **não tem forma fixa**: o de validação traz `field` e `rule`, o de
credencial traz só `message`, o de rate limit traz `retryAfter`. A tarefa **3.6** precisa cobrir
os três, não só o de validação.

Note também que `POST /api/auth/login` com senha errada responde **400**, não 401.

### Achado de segurança: desativar um usuário não revoga a sessão dele

`PATCH /api/users/:id/status` altera `is_active`, e o login passa a ser barrado — mas o
middleware `auth` valida o **token**, não o `is_active`. Quem já estava logado continua com
acesso normal até o token expirar (`AUTH_ACCESS_TOKEN_EXPIRES_IN`, 7 dias por padrão).

O teste `auth/me-deactivated` fixa o comportamento **atual** para que o porte o reproduza. Se o
time decidir que desativar deve derrubar as sessões, mude o teste primeiro — ele vira a
especificação da correção, e a Fase 2 já garante que ninguém a implemente pela metade.

### Consequências combinadas de D4 + D8 que precisam de atenção

A combinação **schema novo + big-bang** é a de maior risco operacional entre as escolhidas.
Mitigações que deixam de ser opcionais:

- **Fase 4** ganha um entregável extra: o **script de migração de dados** (`backend` SQLite → schema novo), com teste de round-trip sobre uma cópia do banco de produção.
- O migrador precisa preservar o **hash dos access tokens** (D1) — senão o cutover derruba todas as sessões, anulando o ganho de D1/D2.
- **Fase 12.13 (shadow traffic)** é a única rede de proteção antes da troca. Não pular.
- **Runbook de rollback** (12.11) precisa incluir o caminho de volta: restaurar o SQLite do Adonis a partir do snapshot pré-cutover.

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

**Status: ✅ CONCLUÍDA (2026-08-09)** · **Bloqueia:** tudo

- [x] 0.1 — **D1 a D10 decididas** e registradas na seção 3.
- [ ] 0.2 — Congelar o `backend/` durante o port (feature freeze) ou definir processo de sincronização. *Pendente de definição do time.*
- [x] 0.3 — `docs/routes-baseline.txt` + `docs/routes-baseline.json` gerados via `node ace list:routes --json`.
- [x] 0.4 — `docs/openapi-baseline.yml` extraído de `GET /api/swagger` (74 KB, **73 paths**).
- [x] 0.5 — `docs/schema-baseline.sql` extraído do SQLite real (`backend/storage/database/app.sqlite3`).
- [x] 0.6 — Crates do Apêndice B validadas contra o crates.io. Toolchain: **rustc 1.96.0**.
- [x] 0.7 — `docker-compose.test.yml` + `tests-fixtures/` criados e **verificados de ponta a ponta**.

### Achados da Fase 0 (corrigem premissas do inventário inicial)

**0.3 — a contagem real é 87, não 85.**
`node ace list:routes` reporta **91 rotas não-HEAD**: 87 sob `/api` + 3 do Transmit + 1 fallback SPA.
A diferença para os 85 "endpoints lógicos" é que `connections/:id` e `storage-destinations/:id`
respondem a **PUT e PATCH** — dois pares método+rota cada, um handler só.
E `/__transmit/*` não é um wildcard: são 3 rotas concretas
(`GET /events`, `POST /subscribe`, `POST /unsubscribe`).

> **Número de referência para o placar de paridade: 87 pares método+rota sob `/api`.**

**0.5 — o banco real está íntegro e as 14 migrations estão aplicadas.**
Não há drift entre as migrations e o schema em disco. Volume de dados a migrar (D4):

| Tabela | Linhas |
|---|---:|
| `resource_metric_history` | 25.458 |
| `auth_access_tokens` | 3 |
| `audit_logs` | 2 |
| `users` · `connections` · `connection_databases` · `backups` · `storage_destinations` · `system_settings` | 1 cada |

**0.6 — todas as crates existem; duas descobertas mudam o plano.**

- 🟢 **GCS deixou de ser risco alto.** `google-cloud-storage = "1.17.0"` é hoje a biblioteca
  **oficial** do Google para Rust, não mais um port de comunidade. Risco rebaixado de 🔴 para 🟢.
- 💡 **`object_store = "0.14"`** oferece uma abstração única sobre S3, GCS, Azure e local.
  Mapeia quase 1:1 no trait `StorageExplorerAdapter` e pode substituir 3 SDKs por 1 dependência.
  **Avaliar na Fase 8** — SFTP continuaria fora dela, de qualquer forma.

**0.7 — ambiente de teste no ar e com fixtures carregados.**

| Serviço | Porta (127.0.0.1) | Estado verificado |
|---|---|---|
| MySQL 8.4 | 13306 | healthy · 4 customers · `fixture_secondary` criado |
| MariaDB 11.4 | 13307 | healthy · 4 customers |
| PostgreSQL 16 | 15432 | healthy · 4 customers · `fixture_secondary` criado |
| MinIO | 19000 / 19001 | healthy · buckets `backups-primary`, `backups-secondary`, `archives` |
| SFTP | 12222 | healthy · `/home/tester/backups` |
| `docker-target` (alpine) | — | up · alvo descartável para a Fase 9 |

> ⚠️ **Armadilha encontrada:** `--default-authentication-plugin` foi **removida no MySQL 8.4** —
> o server aborta no boot com `unknown variable`. O plugin (`caching_sha2_password`) já é o
> default desde a 8.0, então a flag foi simplesmente retirada do compose.

**Descoberta crítica para D3 (achada ao ler `encryption_service.ts`):**
o `EncryptionService` usa **IV de 16 bytes**, não os 12 padrão do AES-GCM, e usa a
`DB_ENCRYPTION_KEY` **diretamente** como chave de 32 bytes, sem nenhum KDF.
Em Rust isso significa `AesGcm<Aes256, U16>` — o alias pronto `Aes256Gcm` (nonce de 12 bytes)
**não serve**. É o primeiro item da Fase 3.

**Pronto quando:** ~~todas as decisões D1–D10 marcadas e o compose de teste sobe com um comando.~~ ✅

---

## Fase 1 — Harness de contrato

**Duração estimada:** 3–5 dias · **Depende de:** Fase 0

Cria a infraestrutura da suíte black-box. Nenhum teste de endpoint ainda — só o esqueleto.

- [x] 1.1 — `contract-tests/` na raiz (workspace independente, Node + Vitest 3 + `undici`). 36 testes do próprio harness + 12 de contrato.
- [x] 1.2 — Config de target em `src/config.ts`: `CONTRACT_TARGET`, `CONTRACT_BASE_URL`, timeouts, retries, modo de golden, `runId`.
- [x] 1.3 — Cliente HTTP (`src/http.ts`) + sessões (`src/session.ts`): `as('admin'|'member'|'inactive')`, `unauth()`, `withBogusToken()`, `expectStatus()`, `expectGolden()`.
- [x] 1.4 — **Gerência de estado determinístico** — o harness sobe e derruba o próprio servidor, com SQLite descartável por execução. Ver decisão abaixo.
- [~] 1.5 — Seeds compartilhados **via HTTP** (`src/seed.ts`): admin, usuário comum ativo, usuário inativo, conexão MySQL, conexão PostgreSQL, storage local, storage MinIO. **Backups ficaram de fora** — ver lacuna abaixo.
- [x] 1.6 — **Golden files** em `__golden__/`, gravados só a partir do Adonis, com redaction de id/timestamp/token/path. Regravação é byte-idêntica (verificado por md5).
- [x] 1.7 — Matchers tolerantes (`src/shape.ts`) com teste próprio, incluindo um que garante que eles **reprovam** algo.
- [x] 1.8 — Relatório de cobertura (`src/report.ts`) cruzando `docs/routes-baseline.txt` com o rastro do cliente HTTP; `--enforce-coverage` reprova a execução.
- [x] 1.9 — Scripts: `contract:record`, `contract:adonis`, `contract:roco`, `contract:diff`, `contract:coverage`, `contract:selftest`.
- [x] 1.10 — CI em `.github/workflows/contract-tests.yml`, com job dedicado a detectar golden desatualizado.

**Pronto quando:** ~~`pnpm contract:record` grava golden de `GET /api/health` e `pnpm contract:adonis` passa.~~ ✅

### 1.4 — decisão registrada: o harness sobe o próprio servidor

Das três opções listadas originalmente, nenhuma foi adotada como estava. O harness prepara um
diretório por execução, roda `node ace migration:run` contra um SQLite descartável, sobe
`node ace serve --no-hmr` numa porta livre, espera `GET /api/health`, semeia por HTTP e no fim
derruba tudo.

O endpoint `POST /api/__test__/reset` foi descartado por dois motivos:

- **D8** (big-bang) exige o `backend/` congelado; abrir rota nova nele só para teste contraria isso;
- o rate limiter do Adonis usa store **em memória** (`config/limiter.ts`), e o limiter de `auth` é
  de **5 req/min por IP+e-mail**. Um endpoint de reset limparia o banco e deixaria os contadores
  intactos. Reiniciar o processo zera as duas coisas de uma vez.

Consequência prática para a Fase 2: **os tokens são emitidos uma única vez por execução**, no seed.
Uma suíte que fizesse login por teste começaria a tomar 429 no sexto.

### Salvaguarda contra o banco de produção

O backend lê o `.env` da **raiz do repositório**, onde está o caminho do banco de produção. As
variáveis que o harness injeta têm precedência (`process.env` vence — verificado no código de
`@adonisjs/env`), mas o harness não confia nisso em silêncio: depois das migrations ele exige que
o arquivo SQLite tenha nascido dentro de `.contract/<runId>/`. Se não nasceu, aborta antes de
escrever qualquer coisa.

### Lacuna assumida: backups não são semeados

Criar um backup pela API exige um banco de origem vivo (`docker-compose.test.yml`) e um dump real.
Fica para o lote de backups da **Fase 2**, onde há teste consumindo o recurso. Está declarado em
`SeedState.backups` e no README, não escondido.

### Tolerâncias da comparação (1.7)

A comparação é de **formato**, nunca de valor. Cada tolerância é uma decisão:

| Situação | Decisão |
|---|---|
| Ordem das chaves | irrelevante — chaves ordenadas na derivação |
| Valor de id, data, duração | irrelevante — só o tipo importa |
| Tamanho de array | irrelevante — o formato do **item** é comparado |
| Array heterogêneo | itens unificados, não só o item 0 |
| `null` em qualquer dos lados | campo nulável, não conflito |
| Array vazio onde o golden tinha itens | reportado como `unverified-array` |
| Chave **a mais** na resposta | **falha** (`allowExtraKeys` afrouxa caso a caso) |
| `charset` do content-type | irrelevante — só o mime é comparado |

O golden guarda **duas** representações: `response.shape`, derivado do corpo **cru**, que é a
autoridade da comparação; e `response.body`, redigido, só para leitura humana no code review.
Guardar apenas o redigido perderia o contrato — a redação troca tipos (`1` vira `"<id>"`).

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

### Lote 2.1 — Público e Auth (9 rotas) ✅

`tests/public.contract.test.ts`, `auth-register`, `auth-login`, `auth-session`.

- [x] `GET /api/health` · `GET /api/swagger` · `GET /api/docs`
- [x] `GET /api/auth/status` (estado de setup inicial)
- [x] `POST /api/auth/register` — pendente de aprovação, e-mail duplicado, senha fraca, e-mail malformado, corpo vazio, JSON malformado, rate limit `auth`, isolamento por e-mail
- [x] `POST /api/auth/login` — sucesso, senha errada (**400**), usuário inexistente indistinguível, inativo, multi-sessão, rate limit `ip-email` com `retry-after`
- [x] `GET /api/auth/me` · `POST /api/auth/logout` — token válido, malformado, inexistente, revogado, duplo logout, revogação não afeta outras sessões

Sem golden para `/api/swagger`: a spec chega como **string** de milhares de linhas, então o
golden guardaria o texto inteiro e qualquer rota nova viraria um diff enorme sem informação. O
contrato útil já é `docs/openapi-baseline.yml`.

### Lote 2.2 — Users e Audit Logs (5 rotas) ✅

`tests/users.contract.test.ts`, `tests/audit-logs.contract.test.ts`.

- [x] `GET /api/users` — paginação (incl. prova de que a página 2 difere da 1), filtro `active`, admin-only, sem vazar hash
- [x] `PATCH /api/users/:id/status` — toggle nos dois sentidos, auto-desativação bloqueada (400), 404, não-admin → 403 **sem efeito colateral**
- [x] `GET /api/audit-logs` — filtros `action`/`entityType`/`status`, ordenação decrescente, teto de 100 por página, lista vazia = 200
- [x] `GET /api/audit-logs/stats` (incl. não ser capturada por `/:id`) · `GET /api/audit-logs/:id` + 404
- [x] Verificar **efeito colateral**: criar e apagar conexão geram `connection.created` / `connection.deleted` com `entityName` e `status` corretos

### Lote 2.3 — Connections (10 rotas) ✅ · `tests/connections.contract.test.ts`
- [x] `GET /api/connections` — paginação, filtro por type, eager load de `databases`
- [x] `POST /api/connections` — MySQL, MariaDB e PostgreSQL, múltiplos databases, senha nunca serializada
- [x] `GET/PUT/PATCH/DELETE /api/connections/:id` — incl. troca de senha e prova de que `PUT` e `PATCH` batem no mesmo handler
- [x] `POST /api/connections/:id/test` — conexão real com o stack, host morto → 422, status vira `error`
- [x] `POST /api/connections/:id/create-database` — sucesso, duplicado, nome com injeção
- [x] `POST /api/connections/:id/backup` — conexão em erro → 422, 404, 401
- [x] `POST /api/connections/discover-databases` — MySQL e PostgreSQL reais, credencial errada sem vazar a senha tentada
- [x] `GET /api/connections/docker-hosts` — degradação graciosa sem Docker
- [x] `GET /api/connections/:connectionId/backups`
- [x] Extra: `PUT` com lista de databases reduzida **desabilita** em vez de apagar (preserva histórico)

### Lote 2.4 — Backups (6 rotas) ✅ · `tests/backups.contract.test.ts`
- [x] `GET /api/backups` — filtros por status/connection/database, paginação
- [x] `GET /api/backups/:id` · `DELETE /api/backups/:id`
- [x] `GET /api/backups/:id/download` — `Content-Disposition: attachment`, 404
- [x] `POST /api/backups/:id/restore` — alvo inválido nunca responde 200
- [x] `POST /api/backups/import` — **multipart montado à mão**: sem arquivo → 422, extensão `.exe` → 422
- [~] Caminho feliz depende de `mysqldump`/`pg_dump` no PATH — pulado com aviso quando faltam

### Lote 2.5 — Storages + Storage Destinations (20 rotas) ✅ · `tests/storages.contract.test.ts`
- [x] CRUD `/api/storages` (5) — MinIO, local e SFTP; `provider` → `type` legado
- [x] CRUD `/api/storage-destinations` (5+2) — a interface legada usa `type`, não `provider`
- [x] `POST /api/storages/:id/test` — conexão real com o MinIO
- [x] `GET /api/storages/:id/browse` — listagem e path traversal no prefixo
- [x] `DELETE /api/storages/:id/object` — inexistente, 404, 401
- [x] `POST /api/storages/:id/copy` + `GET /api/storages/copy-jobs/:jobId`
- [x] `POST /api/storages/:id/archive` + `GET /api/storages/archive-jobs/:jobId` + `/download`
- [x] **Mascaramento de segredos** cobrado em toda resposta que carrega config
- [x] Update sem `secretAccessKey` **preserva** o segredo anterior (senão o storage falharia só no upload)
- [x] `/space` devolve `200 { data: null }` quando o tipo não suporta medição — não 404

### Lote 2.6 — System (10 rotas) ✅ · `tests/system.contract.test.ts`
- [x] `GET /api/stats` · `GET /api/system/status`
- [x] `GET /api/system/diagnostics` · `/:name/download` · `DELETE /:name` — admin-only e **path traversal** nas duas
- [x] `GET /api/system/containers/resources` · `GET /api/system/resources/history` (incl. `rangeHours` não numérico)
- [x] `GET|PUT /api/system/backup-retention` — GFS, cron inválido → 422, persistência verificada
- [x] `POST /api/system/backup-retention/run`

### Lote 2.7 — Docker Manager (25 rotas) ✅ · `tests/docker.contract.test.ts`
- [x] `GET /api/docker/status`
- [x] Containers (9): list, inspect, logs com todos os filtros, clear logs, start, stop, restart, remove
- [x] Volumes (5): list, inspect, export, backup para storage, remove
- [x] Networks (5): list, inspect, create, connect, disconnect
- [x] Images (4): list, inspect, remove, prune
- [x] Diagnostics (2): `POST /api/docker/diagnostics` + `GET /:jobId`
- [x] Nada destrutivo toca container que não seja da suíte — só ids inexistentes e o alvo dedicado

### Lote 2.8 — SSE e não-HTTP ✅ · `tests/global.contract.test.ts`
- [x] `/__transmit/events` (exige `uid`), `/subscribe`, `/unsubscribe`
- [x] Fallback SPA `GET *`
- [x] Headers globais: `force_json_response`, `x-ratelimit-*` (incl. prova de que o contador decresce)

**Pronto quando:** ~~relatório da tarefa 1.8 mostra **100% das rotas cobertas** e a suíte passa
verde contra o Adonis.~~ ✅ **91/91 rotas · 266 testes · 63 golden files**

### Achados da Fase 2

Sete defeitos ou inconsistências que nenhum teste de status pegaria. Todos estão fixados por um
teste que grava o comportamento **atual** — se o time decidir corrigir, o teste é o lugar de
começar, e aí a suíte garante que o back-roco não implemente a correção pela metade.

| # | Achado | Onde | Impacto no porte |
|---|---|---|---|
| 1 | **D9 estava errada**: a API tem duas famílias de erro, não uma | ver seção de decisões | 3.6 precisa cobrir 3 formatos de item em `errors`, não 1 |
| 2 | **Desativar usuário não revoga a sessão** | `auth/me-deactivated` | decisão de produto pendente |
| 3 | **Booleano muda de tipo JSON** conforme o endpoint: `enabled` e `scheduleEnabled` vêm `0`/`1` do banco e `true`/`false` quando ainda estão em memória | `connections.contract.test.ts` | Sea-ORM devolve `bool` sempre — portar "certo" muda o contrato |
| 4 | **O catch-all `GET /*` captura `/api` desconhecido** e devolve a SPA com `200 text/html` | `global.contract.test.ts` | um endpoint digitado errado quebra o `JSON.parse` do cliente com status de sucesso |
| 5 | **Sem Docker, listar degrada para 200 mas inspecionar quebra com 500** | `docker.contract.test.ts` | as duas famílias precisam de uma escolha consciente |
| 6 | **`GET /api/users` ordena sem desempate** — `created_at` tem resolução de segundos e empates saem em ordem arbitrária | `users.contract.test.ts` | paginação sem garantia; revisar todas as listagens |
| 7 | `POST /api/auth/login` com senha errada responde **400**, não 401 | `auth/login-invalid-credentials` | contrato a reproduzir |

### Decisões de engenharia da suíte

- **Ordem alfabética fixa dos arquivos** (`AlphabeticalSequencer` em `vitest.config.ts`). O
  sequenciador padrão ordena por tamanho e por duração das execuções anteriores; numa suíte que
  compartilha um banco isso torna o resultado irreprodutível e faz os golden mudarem sozinhos.
- **Sonda de capacidades** (`src/capabilities.ts`): antes de rodar, o harness verifica MySQL,
  MariaDB, PostgreSQL, MinIO, SFTP, `mysqldump`, `pg_dump` e Docker. O que falta vira aviso em
  letras grandes no console e `it.skipIf`. Pular em silêncio deixaria a suíte verde exatamente
  onde não mediu nada.
- A capacidade Docker é sondada **pelo `/api/docker/status` do próprio backend**, não pelo CLI: no
  Windows o CLI fala por named pipe e funciona, enquanto o backend procura `/var/run/docker.sock`
  e não acha nada.
- **`notComparedPaths`**: o que a comparação ignora também sai do corpo gravado. São os trechos que
  dependem da máquina — uso de CPU, memória livre, latência — e mantê-los faria o golden mudar a
  cada execução sem que nada de contrato tivesse mudado.

---

## Fase 3 — Fundação do back-roco

**Duração estimada:** 1–2 semanas · **Depende de:** Fase 0 · **Paralela às Fases 1–2**

- [x] 3.1 — ✅ **Bloco `settings:`** nos três `config/*.yaml`, com segredos via `get_env`, tipado em `src/initializers/settings.rs`.
  - a validação roda num **`Initializer` do Loco**, não numa função avulsa: o Adonis derruba o processo no `start/env.ts` quando falta `DB_ENCRYPTION_KEY`, e reproduzir isso é o que evita trocar um erro de configuração óbvio por uma falha semanas depois;
  - a chave é conferida no boot (64 caracteres hex) e o TTL do token é parseado (`7d`/`12h`/`30m`) — sufixo desconhecido é **recusado**, não chutado;
  - os quatro limitadores têm default **no código**, não só no YAML: um arquivo incompleto não pode afrouxar um limite em silêncio.
- [x] 3.2 — ✅ **`EncryptionService` em Rust** — `src/models/encryption.rs`. **Go/no-go de D3 aprovado.**
  - `AesGcm<Aes256, U16>` (o alias `Aes256Gcm` usa nonce de 12 e não serviria);
  - `DB_ENCRYPTION_KEY` usada direto como chave de 32 bytes, sem KDF;
  - `Debug` não renderiza a chave, para que um `{:?}` acidental em log não a vaze;
  - **13 testes**: 9 unitários + 4 de compatibilidade cruzada contra vetores gerados pelo Node (`tests/fixtures/encryption_vectors.json`);
  - **prova com dados reais**: teste `decrypts_real_production_rows` (marcado `#[ignore]`, sem segredo versionado) descriptografou em Rust o `password_encrypted` e o `config_encrypted` **do banco de produção** — 9 e 46 bytes, respectivamente.
- [x] 3.3 — ✅ **scrypt** (D2) — `src/models/password.rs`. Formato PHC `$scrypt$n=16384,r=8,p=1$salt$hash`, base64 padrão **sem padding**, salt 16 bytes, keyLength 64.
  - `verify` deriva com os parâmetros **do hash armazenado**, nunca com os da config — do contrário, subir o custo trancaria todos os usuários para fora;
  - comparação em tempo constante (`subtle`); `needs_rehash` para a política futura;
  - o crate `scrypt` recebe `log2(N)`, o Node recebe `N` — custo que não seja potência de 2 é rejeitado, não arredondado;
  - **15 testes**: 8 unitários + 6 de compatibilidade contra hashes gerados pelo driver real do Adonis + 1 contra produção;
  - **prova com dados reais**: `verifies_against_the_real_production_salt` (`#[ignore]`) derivou com o **salt e os parâmetros do hash real** de `users.password` e bateu byte a byte com o Node — sem nunca precisar da senha real.
- [~] 3.4 — **Auth com token opaco** (D1) — camada de formato **pronta** em `src/models/access_token.rs`; a camada de banco espera a Fase 4.
  - formato `oat_<base64url(id)>.<base64url(secret)>`, com `secret = <seed 40 chars><crc32(seed) decimal>`;
  - coluna `hash` = SHA-256 **hex** do secret; comparação em tempo constante;
  - **14 testes**: 8 unitários + 6 de compatibilidade contra tokens emitidos pelo `AccessToken.createTransientToken` real. Inclui confirmação de que o CRC-32 do `crc32fast` (IEEE) é o mesmo do `@poppinss/utils`;
  - **falta (depende da Fase 4):** consulta à tabela `auth_access_tokens`, checagem de `expires_at`, atualização de `last_used_at`, `abilities`, revogação no logout, e o extractor Axum equivalente ao `middleware.auth()`.
- [~] 3.5 — **Rate limit** — algoritmo pronto em `src/controllers/middlewares/rate_limit.rs`; o limitador **global** já está ligado ao router.
  - **janela fixa**, não deslizante: é o que o `rate-limiter-flexible` faz por baixo do Adonis, e o `Retry-After` de uma deslizante não bateria com o golden gravado;
  - requisição já bloqueada **não incrementa** o contador — bater na porta durante o castigo não pode prolongá-lo;
  - o e-mail entra na chave **normalizado**; sem isso `Admin@X.com` e `admin@x.com` teriam contadores separados e o limitador de login deixaria de limitar;
  - limpeza preguiçosa das entradas expiradas: sem ela, variar o IP transformaria o limitador num vazamento de memória;
  - o IP vem de `X-Forwarded-For` antes do socket — atrás de um proxy, o socket veria sempre o mesmo endereço e o limite viraria global para o mundo inteiro;
  - **falta:** os limitadores `auth`, `strict` e `backup` são *por rota*; entram junto com as rotas que os usam (Fases 5+).
- [x] 3.6 — ✅ **Formato de erro** (D9 **corrigida**) — `src/views/errors.rs`, com `impl IntoResponse`.
  - as **duas** famílias, e não uma: `{ errors: [...] }` do framework e `{ success, message }` dos controllers;
  - o item de `errors` é um enum `untagged` — modelar como struct única com campos `Option` emitiria `"field": null` onde o Adonis **omite** a chave, e o matcher da suíte reprova chave a mais;
  - `error` e chaves ausentes usam `skip_serializing_if`, pelo mesmo motivo;
  - os erros do `validator` são ordenados por campo: a iteração de mapa não é estável e a suíte compara a lista de campos.
- [x] 3.7 — ✅ **`force_json`** em `src/controllers/middlewares/force_json.rs` — força `Accept: application/json` na entrada e converte qualquer resposta não-JSON de `/api` em JSON, preservando o status.
- [~] 3.8 — **Auditoria** — enums e tabelas de tradução em `src/models/audit_log.rs`; a persistência espera a Fase 4.
  - `actionDescription`, `actionIcon` e `statusColor` são **derivados**, nunca gravados — sem as mesmas tabelas a interface fica sem rótulo e sem ícone;
  - teste garante que descrições e ícones são **únicos**: dois iguais seriam indistinguíveis na tela de auditoria, e o erro só apareceria durante um incidente;
  - `failure` mapeia para a cor **`error`** — copiar o status como cor é o engano natural aqui.
- [x] 3.9 — ✅ **`ts-rs` corrigido** — `export_to` passou a `../../frontend/src/bindings/`; os bindings agora caem no `frontend/` real da raiz e o diretório fantasma `back-roco/frontend/` foi removido.
- [x] 3.10 — ✅ **Desbloqueada e feita na Fase 4.** `src/fixtures/{users,storage_destinations,connections,connection_databases}.yaml` espelham os seeds da tarefa 1.5 — admin, usuário comum, usuário pendente, storage local e conexão MySQL apontando para o `docker-compose.test.yml`. Carregam com `cargo loco db seed`. O `seed` do `App` ignora arquivo ausente: os fixtures crescem por fase, e exigir todos desde já tornaria o comando inútil até o fim do porte.
- [x] 3.11 — ✅ **`back-roco/Dockerfile`** (multi-stage com `cargo-chef`, usuário sem privilégios, clientes MySQL/PostgreSQL na imagem) e serviço `back-roco` no `docker-compose.dev.yml`, sob o profile `roco`.
  - sobe **lado a lado** com o Adonis, em porta e banco próprios — é o que viabiliza o tráfego-sombra da 12.13 trocando só `CONTRACT_BASE_URL`;
  - **não** compartilha o SQLite do backend: D4 é schema novo, e apontar os dois para o mesmo arquivo corromperia produção;
  - usa a **mesma** `DB_ENCRYPTION_KEY`, senão os ciphertexts já gravados ficam ilegíveis (D3).

### Achados da Fase 3 (em andamento)

**3.2 — D3 confirmado com dados reais.** Não é só o algoritmo que bate: os ciphertexts que
realmente precisam sobreviver à migração foram descriptografados pela implementação Rust.
O risco nº 1 da tabela de riscos está **eliminado**, não apenas mitigado.

**Snapshots do scaffold falhavam fora de UTC.** `cleanup_user_model()` do `loco_rs` redige a data
até os segundos e deixa o offset de fuso (`DATE-03:00`), então os snapshots do scaffold só
passavam na timezone em que foram gravados. Corrigido com um filtro local
`cleanup_user_model_tz()` em `tests/requests/auth.rs`. **Vale para todo snapshot futuro** — use
esse helper, não o do framework, em qualquer teste que serialize `created_at`/`updated_at`.

**As três primitivas de compatibilidade estão provadas contra dados reais.** D1, D2 e D3 deixaram
de ser risco: criptografia, hash de senha e formato de token foram todos verificados contra o que
está gravado no banco de produção, não apenas contra fixtures. O padrão adotado — gerar vetores
com a implementação Node original e travá-los num `tests/fixtures/*.json` — deve ser repetido em
qualquer outro ponto de compatibilidade binária que apareça.

**⚠️ Descoberta para a Fase 4 — `auth_access_tokens` guarda tempo como inteiro de milissegundos.**
`created_at` e `expires_at` são `1785928191780`, não texto ISO. O migrador de dados (4.9) e as
entidades Sea-ORM precisam tratar isso; ler como `DateTime` direto vai falhar ou, pior, silenciar.

**✅ Resolvido na 3.9 — o `export_to` do `ts-rs` apontava para o lugar errado.**
`src/dtos/common.rs` declarava `export_to = "../frontend/src/bindings/"`, mas o caminho é
resolvido a partir de um diretório `bindings/` implícito dentro do crate, e o resultado ia para
`back-roco/frontend/src/bindings/` em vez do `frontend/` da raiz que o SPA consome. Corrigido
para `../../frontend/src/bindings/`; o diretório fantasma foi removido e os bindings agora são
gerados em `frontend/src/bindings/`.

**O `SettingsInitializer` pagou o próprio custo no primeiro `cargo test`.** Um `get_env` com
default vazio renderiza uma linha sem valor, e o YAML lê isso como `null`, não como string vazia —
`initial_admin_bootstrap_token` chegava nulo e o boot falhava. Sem a validação no boot, o erro
teria aparecido em produção, na primeira tentativa de criar o admin inicial.

**Desvio de padrão registrado (seção 12 do `AGENTS.md`).** Os arquivos novos foram acomodados no
mapa da seção 2 em vez de criar pasta nova na raiz de `src/`:

| Conteúdo | Onde ficou | Por quê |
|---|---|---|
| Formato de erro | `src/views/errors.rs` | é serialização de resposta |
| Enums de auditoria | `src/models/audit_log.rs` | é lógica de domínio de um model, e será a casa da persistência na Fase 4 |
| Rate limit e `force_json` | `src/controllers/middlewares/` | mesma pasta que o `loco_rs` usa para middleware |
| Settings | `src/initializers/settings.rs` | a validação **é** um `Initializer` do framework |

**Pronto quando:** ~~`cargo test` verde, `GET /api/health` responde idêntico ao Adonis no contrato,
e um token emitido pelo Adonis é aceito pelo Rust (D1 = token opaco).~~ ✅ para o que não depende
da Fase 4.

Estado atual da suíte: **117 testes, 0 falhas, 2 ignorados** (os de dados de produção).
`cargo fmt --check` e `cargo clippy --all-targets -- -D warnings` limpos.

A **3.10** foi feita junto com a Fase 4. A **3.4**, a **3.5** e a **3.8** foram feitas na Fase 5,
junto com as rotas que as consomem — ver "Os três itens herdados da Fase 3" logo abaixo da
Fase 5. **A Fase 3 está fechada (11/11).**

---

## Fase 4 — Schema, migrations e entidades

**Duração estimada:** 1 semana · **Depende de:** Fase 3

- [x] 4.1 — ✅ **4 migrations** cobrindo as 9 tabelas, agrupadas por dependência de FK e registradas em `migration/src/lib.rs`.
- [x] 4.2 — ✅ A migration `users` do scaffold foi **substituída**. Ver "Remoção da superfície de auth do scaffold" abaixo.
- [x] 4.3 — ✅ **32 índices nomeados**, todos conferidos pelo comparador da 4.8.
- [x] 4.4 — ✅ Constraints replicadas: unique de `connection_databases`, e as FKs com `CASCADE`/`SET NULL` — cada escolha comentada na migration e coberta por teste.
- [x] 4.5 — ✅ Enums como `text` + `CHECK`, igual ao Knex. `audit_logs.action`/`entity_type` ficaram **TEXT sem CHECK**: a migration `10_relax_audit_logs_enums` do Adonis afrouxou os dois de propósito, porque um valor fora da lista fazia o `INSERT` da auditoria derrubar a operação que ela deveria apenas registrar.
- [x] 4.6 — ✅ `db migrate` + `db entities` — 9 entidades geradas em `src/models/_entities/`.
- [x] 4.7 — ✅ Lógica de domínio em `src/models/`: `default_port`, `dump_command`, `mysql_ssl_args`, `schedule_interval_ms`, `decrypted_password`; `safe_config` e `provider_label`; `mark_as_started/completed/failed`, `promote`, `format_size`, `format_duration`.
- [x] 4.8 — ✅ **`cargo run --bin schema_diff`** — compara **estrutura normalizada**, não texto. Resultado: **nenhuma diferença estrutural**.
- [x] 4.9 — ✅ **`cargo run --bin migrate_data`** — validado contra uma cópia do banco de produção.
- [x] 4.10 — ✅ Testes de entidade em `tests/models/entities.rs` (`#[serial]`), mais os unitários de domínio em `src/models/`.

### Remoção da superfície de auth do scaffold

O scaffold do Loco trazia auth por **JWT** com magic link, reset de senha e verificação de
e-mail, e uma tabela `users` com `pid`, `api_key` e `reset_token`. Nada disso existe nas 91 rotas
do contrato, e a decisão **D1** é token opaco. Manter a tabela obrigaria a carregar os dois
schemas ao mesmo tempo e o diff da 4.8 nunca fecharia.

Removidos: `controllers/auth.rs`, `views/auth.rs`, `mailers/auth*`, `tasks/user_create.rs`,
`tests/requests/auth.rs`, `tests/models/users.rs` e os snapshots correspondentes. **Está tudo no
git** se algum trecho for útil depois.

### 4.8 — o comparador compara estrutura, não texto

O Sea-ORM emite `"id" integer NOT NULL PRIMARY KEY AUTOINCREMENT` onde o Knex emite
`` `id` integer not null primary key autoincrement ``. Um `diff` de texto acusaria as 44 linhas
como diferentes e não diria nada. O comparador normaliza os dois lados e confere tabelas,
colunas, **afinidade** SQLite, nulabilidade, e índices — inclusive se são `UNIQUE`.

Tabelas de controle de migration são ignoradas **com o motivo impresso na saída**: uma exceção
que ninguém vê é uma exceção que ninguém revisa.

**Uma diferença ficou, justificada.** O builder do Sea-ORM emite `datetime_text` (afinidade TEXT)
onde o Knex emite `datetime` (NUMERIC). Declarar o tipo cru igualaria o schema — cheguei a fazer
isso e o diff fechou zerado — mas aí o gerador de entidades deixa de reconhecer a coluna e a
mapeia como `String` em vez de `DateTime`. Preferi segurança de tipo a fidelidade cosmética. Só
seria um problema se alguma coluna guardasse número, e é exatamente por isso que o migrador
converte os inteiros de `auth_access_tokens`.

### 4.9 — o migrador, validado contra o banco real

```
users                      origem=1:1              destino=1:1              ok
auth_access_tokens         origem=3:6              destino=3:6              ok
storage_destinations       origem=1:1              destino=1:1              ok
connections                origem=1:1              destino=1:1              ok
connection_databases       origem=1:1              destino=1:1              ok
backups                    origem=1:1              destino=1:1              ok
audit_logs                 origem=2:3              destino=2:3              ok
system_settings            origem=1:1              destino=1:1              ok
resource_metric_history    origem=24608:302789136  destino=24608:302789136  ok
```

Verificado numa **cópia** do `backend/storage/database/app.sqlite3`:

- **hash dos tokens byte-idêntico** — é o que D1 protege; perdê-lo desloga todo mundo no cutover;
- **hash das senhas byte-idêntico** (D2) e **ciphertexts byte-idênticos** (D3) — copiados como
  estão, sem decifrar; recriptografar colocaria os segredos em memória sem necessidade;
- **24.608 linhas** de `resource_metric_history` em lotes de 500 — carregar tudo funcionaria hoje
  e deixaria de funcionar quando a tabela dobrar;
- **idempotente**: rodado duas vezes, checksums idênticos. O tráfego-sombra da 12.13 exige isso;
- tudo numa **transação só**: um erro na 7ª tabela não pode deixar as 6 primeiras migradas.

Conversão feita: `created_at`, `updated_at`, `last_used_at` e `expires_at` de
`auth_access_tokens` guardam **epoch em milissegundos** (`1785928191780`). Viram ISO na cópia —
sem isso o token não seria lido de volta e preservar o `hash` não serviria de nada. A conversão
recusa valores fora da janela 2000–2100: um `0` viraria 1970, que num `expires_at` parece
expiração válida e descartaria o token sem explicação.

**Um bug que o teste com dados reais pegou:** a primeira versão lia as colunas do destino com a
transação já aberta. O SQLite segura o lock de escrita, a consulta ao catálogo pede outra conexão
do pool, e o programa trava esperando o lock que ele mesmo segura — aparece como "connection pool
timed out", sem pista da causa. Corrigido levantando as colunas antes de abrir a transação.

### Entrada standalone das migrations

`migration/src/main.rs` existe para quebrar uma dependência circular real: `cargo loco db migrate`
precisa que a aplicação compile, a aplicação precisa das entidades geradas, e as entidades só
podem ser geradas de um banco já migrado. Com o binário próprio:

```sh
DATABASE_URL="sqlite://banco.sqlite?mode=rwc" cargo run -p migration -- up
```

**Pronto quando:** ~~`cargo loco db migrate` roda limpo, o diff de schema da 4.8 está vazio (ou
justificado), e todos os testes de model passam.~~ ✅ **142 testes**, diff estrutural vazio.

---

## Fase 5 — Auth, Users, Audit, System básico

**Duração estimada:** 1–2 semanas · **Depende de:** Fase 4 · **Cobre lotes 2.1, 2.2 e parte do 2.6**

- [x] 5.1 — ✅ `controllers/auth.rs`: `status`, `register`, `login`, `me`, `logout`. Os endpoints do scaffold (`verify`, `forgot`, `reset`, `magic-link`) já haviam sido **removidos** na Fase 4 — ver "Remoção da superfície de auth do scaffold".
- [x] 5.2 — ✅ Regra de `is_active`, na ordem certa: senha primeiro (400), conta pendente depois (401).
- [x] 5.3 — ✅ `controllers/users.rs`: `index` paginado admin-only e `toggle_status`.
- [x] 5.4 — ✅ `controllers/audit_logs.rs`: `index` com os seis filtros, `stats` e `show`.
- [x] 5.5 — ✅ `controllers/system.rs` parcial: `stats` e `status`, com CPU/memória/uptime reais.
- [x] 5.6 — ✅ Limitador `auth` em `register` e `login`; o global cobre o resto.
- [x] 5.7 — ✅ Registrado em `src/app.rs` — `cargo loco routes` lista as **12 rotas**.
- [x] 5.8 — ✅ **62 testes de request** em `tests/requests/`, mais os unitários dos models e views.

**Pronto quando:** ~~`BASE_URL=<roco> pnpm contract:test --grep "auth|users|audit"` passa 100%.~~
Os testes Rust cobrem os lotes 2.1, 2.2 e a parte de auditoria do 2.6. A execução da suíte de
contrato **contra o back-roco** exige o servidor de pé com o banco semeado, e o alvo `roco` do
harness entra na Fase 12 (tarefa 12.2) — está registrado lá, não aqui.

Estado da suíte: **284 testes, 0 falhas, 2 ignorados** (os de dados de produção).
`cargo fmt --check` e `cargo clippy --all-targets -- -D warnings` limpos.

### Os três itens herdados da Fase 3

| Item | Onde ficou |
|---|---|
| **3.4** — camada de banco do token | `src/models/auth_access_tokens.rs` (`issue`/`verify`/`revoke`) + o extractor `Authenticated` em `src/controllers/middlewares/auth.rs` |
| **3.5** — limitadores por rota | `src/controllers/middlewares/limiters.rs`, com `enforce` servindo tanto ao global quanto aos de rota |
| **3.8** — persistência da auditoria | `src/models/audit_logs.rs` (`record`, `record_or_warn`, `list_page`, `stats`) |

**A precedência dos cabeçalhos `X-RateLimit-*` inverte entre os dois frameworks.** No Adonis o
middleware escreve os cabeçalhos **antes** de chamar o próximo, então o limitador de rota (o mais
interno) sobrescreve o global. Em Axum a resposta só existe na volta, e a ordem se inverte: o de
rota escreve primeiro. Por isso `enforce` só escreve o cabeçalho que ainda **não** existe. Sem essa
regra, `POST /api/auth/login` responderia `x-ratelimit-limit: 600` em vez de `5`, contrariando o
golden `auth/login-rate-limited`.

**`register` e `login` dividem o mesmo orçamento** de 5/min, porque a chave é
`auth_<ip>_<e-mail>` nos dois. Lojas separadas por rota dariam dez tentativas a quem alternasse
entre as duas — que é exatamente o que um ataque de força bruta faria. Está fixado por teste.

### ACHADO 8 — `normalizeEmail()` reescreve o endereço, não só a caixa

O `registerValidator` e o `loginValidator` do Adonis aplicam `normalizeEmail()`, que por baixo é o
do `validator.js` com as opções default. Ele faz **mais** que baixar a caixa: remove o subendereço
(`+tag`) nos provedores conhecidos e, no Gmail, remove também os pontos do local-part e converte
`googlemail.com` em `gmail.com`.

Isso não é detalhe cosmético. O banco migrado guarda o endereço **já normalizado**: quem se
cadastrou como `j.o.a.o+erp@gmail.com` está gravado como `joao@gmail.com`. Se o back-roco apenas
baixasse a caixa, essa pessoa digitaria o e-mail de sempre e receberia *"Invalid user
credentials"* — sem nenhuma pista do motivo, e com o suporte procurando o problema na senha.

Está reproduzido em `src/models/email.rs`, com as listas de domínio do `validator.js`. A regra do
Yahoo **não é idempotente** (`maria-santos-erp` → `maria-santos` → `maria`), e isso é correto: o
que se normaliza é sempre o que a pessoa **digitou**, nunca o que já está gravado.

### Três desvios do `AGENTS.md`, declarados

**1. Um crate novo: `sysinfo`.** O Loco não expõe métricas de máquina e a `std` não dá acesso a
CPU, memória nem uptime. A alternativa seria devolver números inventados num painel de
monitoramento — pior que a dependência. A Fase 11 reusa o mesmo coletor para
`resource_metric_history`.

**2. Um cache em `static`** em `src/models/system_monitor.rs`. Medir CPU exige duas amostras
separadas por um intervalo; sem cache, cada atualização do painel custaria esse intervalo parado.
O Adonis resolve igual, com TTL de 2 s.

**3. `json_body` em vez do extractor `Json` do Axum.** A rejeição do `Json` responde
`400 text/plain`, e estas rotas respondem `422` no shape do VineJS. Um cliente teria de tratar
dois contratos de erro na mesma rota.

### Duas divergências deliberadas, invisíveis no contrato

**`last_used_at` é gravado depois de conferir o hash, não antes.** O
`DbAccessTokensProvider.verify` do Adonis grava primeiro: qualquer requisição com um token
bem-formado cujo `id` exista provoca um `UPDATE`, mesmo com o segredo errado. A coluna não aparece
em nenhuma resposta, e a ordem do Adonis daria a qualquer anônimo um jeito de gerar escrita no
banco em rajada.

**A listagem de usuários e a de auditoria ganharam `id desc` como desempate.** O Adonis ordena só
por `created_at desc` (ACHADO 6): registros criados no mesmo segundo saem em ordem arbitrária, e a
mesma linha pode aparecer em duas páginas. O desempate não muda a ordenação contratada — só a
torna estável.

### O que ficou de fora, e por quê

- **`storageSpaces` de `GET /api/stats` sai vazio** até a Fase 8 ligar o `StorageSpaceService`.
  Sai como `[]`, e não omitido: o painel itera sobre ele.
- **`jobs.status` responde `down`** até o scheduler da Fase 10. É a verdade; um `ok` otimista
  esconderia do painel que o agendador não existe.
- **As outras oito rotas de `system`** (`diagnostics`, `containers/resources`,
  `resources/history`, `backup-retention`) dependem do cliente Docker da Fase 9 e da política de
  retenção da Fase 11.
- **`GET /api/audit-logs` não exige administrador**, porque o Adonis não exige. Restringir faria
  sentido, mas é mudança de comportamento escondida dentro de um porte — quem decide é o produto.
- **Timestamps saem sem fuso** (`2026-08-06T16:49:25.000`). O banco guarda hora local ingênua e o
  deslocamento original não está gravado em lugar nenhum; escrever `Z` afirmaria que aquilo é UTC
  e o navegador renderizaria o backup das 16h49 como 13h49.

---

## Fase 6 — Connections + drivers de banco

**Duração estimada:** 2–3 semanas · **Depende de:** Fase 5 · **Cobre lote 2.3**

- [x] 6.1 — ✅ CRUD completo + reconciliação de `connection_databases`.
- [x] 6.2 — ✅ Validação em `src/models/validation.rs`, compartilhada com `users`.
- [x] 6.3 — ✅ **`sqlx`** com as features `mysql` e `postgres`, em `src/models/database_driver.rs`.
- [x] 6.4 — ✅ `POST /:id/test`, gravando `status`/`last_error`/`last_tested_at` nos dois desfechos.
- [x] 6.5 — ✅ `POST /discover-databases`, com os mesmos filtros de bancos de sistema.
- [x] 6.6 — ✅ `POST /:id/create-database` — **duas** barreiras contra injeção em DDL.
- [x] 6.7 — ✅ Stub: **200** com `dockerAvailable: false` e lista vazia, que é o corpo que o Adonis devolve numa máquina sem Docker.
- [ ] 6.8 — ⏸️ `connection_port_selection_resolver`, `connection_suggestion_mapper`, `container_port_resolver` e `network_reachability_resolver` **só têm sentido com o cliente Docker**: os quatro operam sobre a lista de containers. Movidos para a Fase 9, junto de quem os alimenta.
- [x] 6.9 — ✅ `connection.created/updated/deleted/tested`, com IP e agente da requisição.
- [x] 6.10 — ✅ **20 testes de request** + os unitários de model, view e driver.

**Pronto quando:** ~~lote 2.3 do contrato passa contra o `back-roco` com MySQL, MariaDB e PG
reais.~~ Passa o que não depende de servidor real. Dois testes ficaram **`#[ignore]`d**, lendo
`CONTRACT_MYSQL_PORT` do ambiente — rodam com o compose da 0.7 de pé
(`cargo test --test mod -- --ignored`). Ficam ignorados por padrão porque uma suíte que falha por
ambiente ausente deixa de ser lida.

Estado da suíte: **329 testes, 0 falhas, 6 ignorados**. `fmt` e `clippy -D warnings` limpos,
**22 rotas** em `cargo loco routes`.

### O contrato tem três formas de conexão, não uma

O Lucid serializa o objeto que estiver na memória, e o que está na memória depende de como ele
chegou lá. Os goldens registram a diferença:

| Rota | `scheduleEnabled` | `lastError`/`lastTestedAt`/`lastBackupAt` | `backups` |
|---|---|---|---|
| `POST` (store) | `false` — booleano | **ausentes** | ausente |
| `PUT` (update) | `0` — número | presentes | ausente |
| `GET` (index/show) | `0` — número | presentes | presente |

No `store` o registro nunca voltou do banco: os três campos jamais foram atribuídos, e
`JSON.stringify` **omite** `undefined`. Nas outras rotas o registro veio do SQLite, onde booleano
é `0`/`1`, e o model de `connections` — diferente do de `users` — não tem `consume` convertendo de
volta. É o **ACHADO 3**, e emitir `true` onde o contrato diz `1` quebra todo cliente que compare
com `===`.

Reproduzido com o enum `WireBool` em `src/views/connections.rs`, que obriga cada view a **escolher
explicitamente** o tipo JSON. Um `bool` puro emitiria `true` em toda parte e a divergência só
apareceria no cliente.

### `meta.choices`: um quarto formato de item de erro

O golden `connections/store-invalid-type` mostra que o erro de `vine.enum()` carrega
`meta.choices` com a lista aceita — é com ela que a interface remonta o select depois de um 422.
Virou uma variante própria de `ErrorItem`, e não um campo opcional na de validação: opcional
emitiria `"meta": null` em **todo** erro de validação.

### Remover um database desabilita, não apaga

O `PUT` marca `enabled = false` nos nomes que saíram da lista. O motivo está na FK de
`backups.connection_database_id`: apagar a linha levaria junto o histórico de backups daquele
banco — que é justamente o que alguém consulta depois de remover um database por engano.
Readicionar o nome **reativa** a linha antiga; criar outra esbarraria no índice único
`idx_conn_db_unique`.

### Duas barreiras contra injeção em DDL

`CREATE DATABASE` não aceita parâmetro em nenhum dos motores — o nome entra na string. A primeira
barreira é a validação do controller (`^[A-Za-z_][A-Za-z0-9_-]*$`, igual ao
`createDatabaseValidator`); a segunda é `quote_identifier` no driver, que revalida e aspa. Ambas
são **lista de permissão**: escapar aspas bastaria na teoria, mas uma lista de bloqueio erra em
silêncio no dia em que aparece um caractere que ninguém previu.

### Uma consulta, não uma por linha

`GET /api/connections` precisa do backup mais recente de **cada** conexão da página — o
`groupLimit(1)` do Lucid. Um `SELECT` por conexão dentro do laço seria uma ida ao banco por linha;
carregar todos os backups e filtrar em memória traria milhares de linhas para descartar quase
todas. Resolvido com `ROW_NUMBER() OVER (PARTITION BY connection_id …)`, que o SQLite suporta
desde a 3.25.

### Três divergências deliberadas

**`create-database` usa o driver, não o CLI.** O `DatabaseManagementService` do Adonis dá `spawn`
em `mysql`/`psql` — tanto que o golden `create-database-duplicate` gravou `spawn mysql ENOENT`,
que é o erro de a máquina não ter o binário. Pelo driver não há dependência externa nem processo
filho, e o status (422) é o mesmo. A mensagem muda, e para melhor.

**PostgreSQL negocia TLS quando o servidor oferece** (`sslMode=Prefer`), em vez de sempre texto
claro. É o mesmo conjunto de servidores que o `pg` do Adonis aceita, com criptografia onde dá.
MySQL segue desligado salvo `options.ssl`, igual ao `--skip-ssl` que o model já passava ao
`mysqldump` — exigir TLS por padrão derrubaria conexões que hoje funcionam.

**A listagem ganhou `id asc` como desempate** de `name asc`, pelo mesmo motivo das listagens da
Fase 5: sem ele, duas conexões de mesmo nome podem aparecer duas vezes numa página e sumir de
outra.

### O que ficou de fora

~~`POST /api/connections/:id/backup` está na rota de `connections`, mas o corpo dela é o pipeline de
dump — **Fase 7**.~~ ✅ **Feita na Fase 7**, no `controllers/connections.rs`, com as guardas de
`status = error` e "nenhum database habilitado" que o golden
`connections/backup-connection-in-error` fixa.

---

## Fase 7 — Backups, dump e restore

**Duração estimada:** 3–4 semanas · **Depende de:** Fase 6 · **Cobre lote 2.4** · 🔴 **Maior risco**

- [x] 7.1 — ✅ `controllers/backups.rs`: `index`, `by_connection`, `show`, `destroy`, `download`, `restore`, `import`. Mais `POST /api/connections/:id/backup`, herdado da Fase 6.
- [x] 7.2 — ✅ **Pipeline de dump** em `src/models/dump.rs`. `tokio::process::Command`, `stdout → sha256 → gzip → arquivo` num `tokio::io::copy`, sem bufferizar.
- [x] 7.3 — ✅ SHA-256 em streaming, **antes** do gzip, via o adaptador `HashingWriter`.
- [x] 7.4 — ✅ `src/models/process_output.rs`, teto de 256 KB com sufixo de truncamento. O porte de `child_process_exit` **não** era necessário — ver abaixo.
- [x] 7.5 — ✅ `GET /:id/download` com `ReaderStream`, `Content-Disposition: attachment` e `Content-Length`. Storage remoto fica para a Fase 8.
- [x] 7.6 — ✅ **Restore** em `src/models/restore.rs`, com os filtros trabalhando em **bytes**. Os casos de `restore_filters.spec.ts` foram portados.
- [x] 7.7 — ✅ **Import** em `src/models/backup_import.rs` + o handler multipart: extensão, magic bytes, integridade opcional, checksum e gravação com sufixo `.part`.
- [x] 7.8 — ✅ `src/models/progress.rs` — `ProgressHub` sobre `tokio::sync::broadcast`, com os dois emissores e o estrangulamento de 500 ms. A Fase 10 troca o assinante, não os emissores.
- [~] 7.9 — Remoção **local** feita; a do objeto no storage remoto depende dos adaptadores da Fase 8. O `DELETE` emite aviso explícito em vez de silenciar — ver abaixo.
- [x] 7.10 — ✅ `Backup::can_be_deleted()` bloqueia `protected` **e** backup em andamento, com teste para os dois.
- [x] 7.11 — ✅ `backup.started/completed/failed/deleted/downloaded/imported`, todos com IP e agente da requisição.
- [x] 7.12 — ✅ **32 testes de request** em `tests/requests/backups.rs`, mais os unitários de `dump`, `restore`, `backup_import`, `progress`, `process_output`, `backup_storage` e das views.

**Pronto quando:** ~~um backup real de MySQL e de PG é gerado, baixado, restaurado e validado
byte-a-byte contra o resultado do Adonis.~~ Passa o que não depende de servidor real **nem de
`mysqldump`/`pg_dump` no PATH**. A comparação byte-a-byte contra o Adonis exige as duas coisas e o
compose da 0.7 de pé; ela entra na **12.2** (diff automatizado Adonis × Roco), onde já existe a
infraestrutura para rodar os dois lado a lado — está registrada lá, não aqui.

Estado da suíte: **457 testes, 0 falhas, 4 ignorados** (2 de dados de produção, 2 de servidor
real). `cargo fmt --check` e `cargo clippy --all-targets -- -D warnings` limpos, **30 rotas** em
`cargo loco routes`.

### Onde cada peça ficou

O `AGENTS.md` proíbe criar pasta nova na raiz de `src/` e proíbe uma camada genérica de
"service"/"repository". Os seis módulos novos foram acomodados no mapa existente, seguindo o
critério já usado por `models/database_driver.rs` e `models/system_monitor.rs`:

| Conteúdo | Onde ficou | Por quê |
|---|---|---|
| Pipeline de dump | `src/models/dump.rs` | lógica de domínio, sem HTTP |
| Filtros e pipeline de restore | `src/models/restore.rs` | idem |
| Detecção de formato e integridade | `src/models/backup_import.rs` | idem |
| Orquestração (registro + dump + destino) | `src/models/backup_runner.rs` | o fio entre os anteriores e o banco de controle |
| Resolução de caminho e destino | `src/models/backup_storage.rs` | parte local do `storage_destination_service` |
| Captura de saída de processo | `src/models/process_output.rs` | utilitário de domínio |
| Emissores de progresso | `src/models/progress.rs` | o hub vive no `shared_store` do `AppContext` |
| Worker de restauração | `src/workers/restore.rs` | é o gancho do framework para trabalho assíncrono |

**Duas dependências novas, declaradas.** `async-compression` (gzip em streaming) e `tokio-util`
(`ReaderStream` para o download). O Loco não entrega compressão de arquivo; a alternativa —
`flate2` num `spawn_blocking` — exigiria bombear os pedaços à mão entre o mundo async e o
síncrono, e perderia o backpressure de ponta a ponta.

### Achados da Fase 7

**`child_process_exit.ts` não tinha o que portar.** Aquele helper existe porque, no Node, `spawn()`
não falha na hora: um binário ausente vira um evento `error` assíncrono que **corre** com o evento
`close` e pode ser perdido. Em Rust, `tokio::process::Command::spawn()` devolve `Err`
imediatamente, e `child.wait()` devolve o código de saída. A corrida que o helper resolvia não
existe aqui — está registrado no doc do `process_output.rs` para que ninguém o "porte" depois por
simetria.

**O bug de UTF-8 do restore desaparece trabalhando em bytes.** O `RestoreService` do Adonis usa um
`StringDecoder` para não partir um caractere multibyte na fronteira entre chunks — foi o que os
testes `restore_filters.spec.ts` fixaram, e a corrupção era silenciosa: acentuação errada no banco
restaurado, sem nenhum erro. Aqui as regras decidem sobre `&[u8]` e o único separador procurado é
`\n` (`0x0A`), que **não aparece** dentro de nenhuma sequência UTF-8. Não há conversão, não há
fronteira para errar — e, de quebra, um dump em Latin-1 passa intacto, onde o decodificador do Node
o teria substituído por `U+FFFD`.

**O `stderr` precisa ser drenado em paralelo, não depois.** Lê-lo após o fim do processo trava o
dump assim que o pipe de erro enche — e qualquer `mysqldump` com muitos avisos enche. As duas
saídas vão para `tokio::spawn` antes de a escrita começar.

**O checksum é dos bytes descomprimidos, e isso não é detalhe.** O cabeçalho do gzip carrega
timestamp: comprimir o mesmo dump duas vezes produz arquivos diferentes. Um checksum do `.gz` nunca
bateria entre duas execuções e seria inútil para verificar integridade. O `HashingWriter` fica
**antes** do encoder, como no Adonis.

**O extractor `Multipart` do Axum precisou de `Result`.** A rejeição dele responde `400 text/plain`,
e o golden `backups/import-no-file` grava **422** com o corpo da família dos controllers. É o mesmo
motivo pelo qual `json_body` não usa o extractor `Json` (registrado na Fase 5): um cliente que
errasse o `Content-Type` receberia um contrato de erro que não existe em nenhuma outra rota.

**O parâmetro de rota tem que ter o mesmo nome na mesma posição.** `GET /api/connections/:connectionId/backups`
foi registrada como `/{id}/backups`, e não `/{connection_id}/backups`: o roteador do Axum entra em
pânico no boot com "conflicting route" quando duas rotas do mesmo prefixo usam nomes diferentes
para o parâmetro na mesma posição. O nome do parâmetro não aparece no contrato; o pânico apareceria
só no boot de produção.

**Um defeito encontrado e corrigido no adaptador de filtro.** A primeira versão devolvia os bytes
não aceitos pelo `stdin` para o buffer de linha. O `stdin` de um processo aceita escrita parcial —
o pipe tem buffer finito e o `psql` consome no ritmo dele —, então o resto seria **filtrado uma
segunda vez**: linhas já aprovadas passariam de novo pelas regras, e um bloco `COPY` cortado no
meio mudaria o estado do filtro. Corrigido com uma fila de saída separada (`outbox`), que guarda o
que já foi aprovado mas ainda não foi aceito.

**`inferDatabaseName` tem um quirk que foi preservado.** O padrão `[_-]?\d{8,}[_-]?` consome os
separadores dos **dois** lados do bloco de 8+ dígitos, e a hora (6 dígitos) fica colada:
`vendas_20260809_120000.sql.gz` vira `vendas120000`, não `vendas`. O valor é só o default de
`databaseName` quando o formulário não o envia, mas aparece na listagem — "corrigir" mudaria o que
fica gravado em todo backup importado sem nome. Está fixado por teste, com o motivo escrito.

### Três divergências deliberadas

**`clearBeforeRestore` usa o driver, não o CLI.** O Adonis dá `spawn` em `psql`/`mysql` só para
rodar três comandos de DDL. Pelo `database_driver` não há processo filho nem dependência de binário
no PATH — mesma escolha já registrada na Fase 6 para `create-database`. O restore em si continua
pelo CLI: nenhum driver executa um dump com centenas de milhares de instruções de forma confiável.

**A restauração roda num worker do Loco, não num `tokio::spawn`.** O `spawn` solto some do radar:
não respeita o `workers.mode` configurado e, no ambiente de teste (`ForegroundBlocking`), rodaria
em paralelo com as asserções em vez de antes delas. O worker é o gancho que o framework já oferece
— e o `perform` devolve `Err` **só** para falha de infraestrutura, porque devolver `Err` para uma
restauração que falhou faria a fila tentar de novo e restaurar o banco duas vezes.

**A listagem ganhou teto de 100 itens por página.** O `BackupsController` do Adonis não tem teto, e
`?limit=1000000` seria um jeito barato de derrubar o processo pela memória. É a mesma proteção que
`GET /api/audit-logs` já aplica, e o contrato só fixa o comportamento com valores razoáveis.

### O que ficou de fora, e por quê

- **Upload e download em storage remoto.** O dump é gravado localmente e o `DELETE` remove só a
  cópia local. Os dois pontos emitem `tracing::warn!` explícito nomeando a Fase 8 — "o backup foi
  para o S3" nunca pode ser uma suposição de quem lê o log, e um `DELETE` silencioso deixaria o
  objeto no bucket para sempre. É a pendência **7.9**.
- **Os eventos de progresso não saem pelo SSE** — vão para o canal interno, como a própria 7.8
  prevê. A 10.3 pluga o assinante.
- **`MYSQL_PWD` não substituiu `--password=`.** O `mysqldump` recebe a senha na linha de comando,
  onde ela aparece no `ps` da máquina — é o que o Adonis faz, e trocar mudaria comportamento
  observável que nenhum teste de contrato cobre. Fica registrado para a revisão de segurança da
  **12.8** decidir, em vez de mudar em silêncio dentro de um porte.
- **`pg_dump -Fc` continua sendo só um formato de importação.** O dump gerado pelo back-roco é
  sempre `.sql.gz`, igual ao do Adonis.

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
- [ ] 12.2 — Diff automatizado Adonis × Roco: rodar a suíte contra os dois em paralelo e comparar respostas campo a campo. **Inclui a validação byte-a-byte do backup/restore herdada da Fase 7** — ela exige `mysqldump`/`pg_dump` no PATH e o compose da 0.7, e é aqui que os dois lados rodam lado a lado.
- [ ] 12.3 — Swagger conforme D10 — comparar com `openapi-baseline.json`.
- [ ] 12.4 — Fallback SPA + `@adonisjs/static` equivalente (servir `public/`).
- [ ] 12.5 — **Frontend contra o `back-roco`** — rodar a suíte E2E do frontend sem nenhuma alteração de código.
- [ ] 12.6 — **Benchmark** — latência p50/p95/p99 e uso de memória nos endpoints quentes (listagens, browse, métricas). Documentar o ganho.
- [ ] 12.7 — Teste de carga nos jobs assíncronos (backup de banco grande, archive de bucket grande).
- [ ] 12.8 — Revisão de segurança do diff: sem segredo em log, sem `unwrap` em handler, criptografia validada, path traversal coberto em todos os pontos.
- [ ] 12.9 — `cargo fmt` + `cargo clippy --all-targets -- -D warnings` limpos.
- [ ] 12.10 — Dockerfile de produção multi-stage + entrada no `docker-compose.yml`.
- [ ] 12.11 — Documentação: README, AGENTS.md atualizado, guia de migração, runbook de rollback.
- [ ] 12.12 — **Cutover big-bang** (D8): snapshot do SQLite → rodar o migrador da 4.9 → subir o `back-roco` → derrubar o Adonis. Janela de downtime planejada e comunicada.
- [ ] 12.13 — **Shadow traffic (obrigatório)** — rodar os dois em paralelo, espelhar o tráfego real para o `back-roco`, comparar respostas, servir só o Adonis. É a única rede de proteção antes de um big-bang.
- [ ] 12.14 — Descomissionar o `backend/` só após N dias de estabilidade. Manter o snapshot pré-cutover durante todo o período.

---

## Resumo de esforço

| Fase | Escopo | Estimativa | Risco | Estado |
|---|---|---|---|---|
| 0 | Inventário e decisões | 2–3 dias | 🟢 | ✅ concluída |
| 1 | Harness de contrato | 3–5 dias | 🟢 | ✅ concluída |
| 2 | 85 testes de endpoint | 3–4 semanas | 🟡 | ✅ concluída |
| 3 | Fundação | 1–2 semanas | 🟡 | ✅ concluída |
| 4 | Schema e migrations | 1 semana | 🟡 | ✅ concluída |
| 5 | Auth/Users/Audit/System | 1–2 semanas | 🟢 | ✅ concluída |
| 6 | Connections + drivers | 2–3 semanas | 🟡 | ✅ concluída |
| 7 | Backups/dump/restore | 3–4 semanas | 🔴 | ✅ concluída (7.9 parcial → Fase 8) |
| 8 | Storages multi-provider | 3–4 semanas | 🔴 | ⬜ próxima |
| 9 | Docker Manager | 2–3 semanas | 🔴 | ⬜ pode entrar em paralelo |
| 10 | SSE/scheduler/workers | 2 semanas | 🟡 | ⬜ pode entrar em paralelo |
| 11 | System avançado | 1–2 semanas | 🟡 | ⬜ depende de 9 e 10 |
| 12 | Paridade e cutover | 2–3 semanas | 🟡 | ⬜ |

**Total sequencial: ~5–7 meses.** Com as Fases 1–2 em paralelo às 3–4, e a Fase 9 em paralelo
às 6–8, cai para **~4–5 meses** com duas frentes de trabalho.

### O que a Fase 8 herda

Quatro pendências fecham quando os adaptadores de storage existirem. Nenhuma delas é surpresa: as
três da Fase 7 avisam em tempo de execução, e a da Fase 5 já estava registrada lá.

| Herdada de | O quê | Onde está sinalizado |
|---|---|---|
| 7.9 | Remover o objeto no destino remoto ao apagar um backup | `tracing::warn!` no `DELETE /api/backups/:id` |
| 7.2 | Enviar o dump para o destino remoto depois de gravá-lo | `tracing::warn!` ao fim de `run_backup` |
| 7.6 | Baixar o arquivo de um destino remoto para restaurar | mensagem de erro nomeando a Fase 8 |
| 5.x | `storageSpaces` de `GET /api/stats` sai como `[]` | registrado na Fase 5 |

---

## Apêndice A — Matriz completa de endpoints

Legenda: **T** = teste de contrato escrito (Fase 2) · **P** = portado no `back-roco` ·
**V** = verde contra o Roco. Valores: ✅ pronto · 🟡 parcial (o motivo está na fase correspondente) ·
⬜ não começou.

**Placar de paridade — atualizado ao fim da Fase 7:**

| | Pares método+rota | % |
|---|---:|---:|
| **T** — teste de contrato escrito | 87 / 87 | 100% |
| **P** — portado (28 ✅ + 2 🟡) | 30 / 87 | 34% |
| **V** — verde contra o Roco | 0 / 87 | 0% |

A coluna **V** só começa a mudar na **12.1**: rodar a suíte de contrato contra o `back-roco` exige
o alvo `roco` do harness, que é a tarefa **12.2**. Marcar V antes disso seria afirmar paridade
verificada com base nos testes Rust, que medem outra coisa — eles são a rede interna, não o
contrato.

> As linhas 14 e 22 valem **dois** pares método+rota cada (`PUT` e `PATCH` no mesmo handler), e é
> por isso que 30 linhas marcadas em P correspondem aos 30 caminhos que `cargo loco routes` lista
> hoje sob `/api`.

### Público

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 1 | GET | `/api/health` | inline | global | ✅ | ⬜ | ⬜ |
| 2 | GET | `/api/swagger` | autoswagger | global | ✅ | ⬜ | ⬜ |
| 3 | GET | `/api/docs` | autoswagger | global | ✅ | ⬜ | ⬜ |
| 4 | GET | `/api/auth/status` | Auth.checkStatus | global | ✅ | ✅ | ⬜ |
| 5 | POST | `/api/auth/register` | Auth.register | auth (ip-email) | ✅ | ✅ | ⬜ |
| 6 | POST | `/api/auth/login` | Auth.login | auth (ip-email) | ✅ | ✅ | ⬜ |

### Auth protegido

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 7 | GET | `/api/auth/me` | Auth.me | global | ✅ | ✅ | ⬜ |
| 8 | POST | `/api/auth/logout` | Auth.logout | global | ✅ | ✅ | ⬜ |

### Connections

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 9 | POST | `/api/connections/discover-databases` | Connections.discoverDatabases | strict | ✅ | ✅ | ⬜ |
| 10 | GET | `/api/connections/docker-hosts` | Connections.dockerHosts | global | ✅ | 🟡 | ⬜ |
| 11 | GET | `/api/connections` | Connections.index | global | ✅ | ✅ | ⬜ |
| 12 | POST | `/api/connections` | Connections.store | global | ✅ | ✅ | ⬜ |
| 13 | GET | `/api/connections/:id` | Connections.show | global | ✅ | ✅ | ⬜ |
| 14 | PUT/PATCH | `/api/connections/:id` | Connections.update | global | ✅ | ✅ | ⬜ |
| 15 | DELETE | `/api/connections/:id` | Connections.destroy | global | ✅ | ✅ | ⬜ |
| 16 | POST | `/api/connections/:id/test` | Connections.test | strict | ✅ | ✅ | ⬜ |
| 17 | POST | `/api/connections/:id/create-database` | Connections.createDatabase | strict | ✅ | ✅ | ⬜ |
| 18 | POST | `/api/connections/:id/backup` | Connections.backup | backup | ✅ | ✅ | ⬜ |

### Storage Destinations (legado)

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 19 | GET | `/api/storage-destinations` | StorageDestinations.index | global | ✅ | ⬜ | ⬜ |
| 20 | POST | `/api/storage-destinations` | StorageDestinations.store | global | ✅ | ⬜ | ⬜ |
| 21 | GET | `/api/storage-destinations/:id` | StorageDestinations.show | global | ✅ | ⬜ | ⬜ |
| 22 | PUT/PATCH | `/api/storage-destinations/:id` | StorageDestinations.update | global | ✅ | ⬜ | ⬜ |
| 23 | DELETE | `/api/storage-destinations/:id` | StorageDestinations.destroy | global | ✅ | ⬜ | ⬜ |
| 24 | GET | `/api/storage-destinations-space` | StorageDestinations.spaceAll | global | ✅ | ⬜ | ⬜ |
| 25 | GET | `/api/storage-destinations/:id/space` | StorageDestinations.space | global | ✅ | ⬜ | ⬜ |

### Storages

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 26 | GET | `/api/storages` | Storages.index | global | ✅ | ⬜ | ⬜ |
| 27 | POST | `/api/storages` | Storages.store | global | ✅ | ⬜ | ⬜ |
| 28 | GET | `/api/storages/copy-jobs/:jobId` | Storages.copyStatus | global | ✅ | ⬜ | ⬜ |
| 29 | GET | `/api/storages/archive-jobs/:jobId` | Storages.archiveJobStatus | global | ✅ | ⬜ | ⬜ |
| 30 | GET | `/api/storages/archive-jobs/:jobId/download` | Storages.downloadArchive | global | ✅ | ⬜ | ⬜ |
| 31 | GET | `/api/storages/:id` | Storages.show | global | ✅ | ⬜ | ⬜ |
| 32 | PUT | `/api/storages/:id` | Storages.update | global | ✅ | ⬜ | ⬜ |
| 33 | DELETE | `/api/storages/:id` | Storages.destroy | global | ✅ | ⬜ | ⬜ |
| 34 | POST | `/api/storages/:id/test` | Storages.test | strict | ✅ | ⬜ | ⬜ |
| 35 | GET | `/api/storages/:id/browse` | Storages.browse | global | ✅ | ⬜ | ⬜ |
| 36 | DELETE | `/api/storages/:id/object` | Storages.destroyObject | global | ✅ | ⬜ | ⬜ |
| 37 | POST | `/api/storages/:id/copy` | Storages.startCopy | backup | ✅ | ⬜ | ⬜ |
| 38 | POST | `/api/storages/:id/archive` | Storages.startArchive | backup | ✅ | ⬜ | ⬜ |

### Backups

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 39 | GET | `/api/backups` | Backups.index | global | ✅ | ✅ | ⬜ |
| 40 | GET | `/api/connections/:connectionId/backups` | Backups.byConnection | global | ✅ | ✅ | ⬜ |
| 41 | GET | `/api/backups/:id` | Backups.show | global | ✅ | ✅ | ⬜ |
| 42 | GET | `/api/backups/:id/download` | Backups.download | global | ✅ | ✅ | ⬜ |
| 43 | POST | `/api/backups/:id/restore` | Backups.restore | strict | ✅ | ✅ | ⬜ |
| 44 | DELETE | `/api/backups/:id` | Backups.destroy | global | ✅ | ✅ | ⬜ |
| 45 | POST | `/api/backups/import` | Backups.import | backup | ✅ | ✅ | ⬜ |

### System

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 46 | GET | `/api/stats` | System.stats | global | ✅ | 🟡 | ⬜ |
| 47 | GET | `/api/system/status` | System.status | global | ✅ | ✅ | ⬜ |
| 48 | GET | `/api/system/diagnostics` | System.diagnostics | global | ✅ | ⬜ | ⬜ |
| 49 | GET | `/api/system/diagnostics/:name/download` | System.downloadDiagnostic | strict | ✅ | ⬜ | ⬜ |
| 50 | DELETE | `/api/system/diagnostics/:name` | System.destroyDiagnostic | strict | ✅ | ⬜ | ⬜ |
| 51 | GET | `/api/system/containers/resources` | System.containerResources | global | ✅ | ⬜ | ⬜ |
| 52 | GET | `/api/system/resources/history` | System.resourcesHistory | global | ✅ | ⬜ | ⬜ |
| 53 | GET | `/api/system/backup-retention` | System.backupRetentionPolicy | global | ✅ | ⬜ | ⬜ |
| 54 | PUT | `/api/system/backup-retention` | System.updateBackupRetentionPolicy | strict | ✅ | ⬜ | ⬜ |
| 55 | POST | `/api/system/backup-retention/run` | System.runBackupRetention | strict | ✅ | ⬜ | ⬜ |

### Audit Logs

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 56 | GET | `/api/audit-logs` | AuditLogs.index | global | ✅ | ✅ | ⬜ |
| 57 | GET | `/api/audit-logs/stats` | AuditLogs.stats | global | ✅ | ✅ | ⬜ |
| 58 | GET | `/api/audit-logs/:id` | AuditLogs.show | global | ✅ | ✅ | ⬜ |

### Users

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 59 | GET | `/api/users` | Users.index | global | ✅ | ✅ | ⬜ |
| 60 | PATCH | `/api/users/:id/status` | Users.toggleStatus | global | ✅ | ✅ | ⬜ |

### Docker — Containers

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 61 | GET | `/api/docker/status` | DockerManager.status | global | ✅ | ⬜ | ⬜ |
| 62 | GET | `/api/docker/containers` | DockerManager.listContainers | global | ✅ | ⬜ | ⬜ |
| 63 | GET | `/api/docker/containers/:id` | DockerManager.inspectContainer | global | ✅ | ⬜ | ⬜ |
| 64 | GET | `/api/docker/containers/:id/logs` | DockerManager.containerLogs | global | ✅ | ⬜ | ⬜ |
| 65 | DELETE | `/api/docker/containers/:id/logs` | DockerManager.clearContainerLogs | strict | ✅ | ⬜ | ⬜ |
| 66 | POST | `/api/docker/containers/:id/start` | DockerManager.startContainer | strict | ✅ | ⬜ | ⬜ |
| 67 | POST | `/api/docker/containers/:id/stop` | DockerManager.stopContainer | strict | ✅ | ⬜ | ⬜ |
| 68 | POST | `/api/docker/containers/:id/restart` | DockerManager.restartContainer | strict | ✅ | ⬜ | ⬜ |
| 69 | DELETE | `/api/docker/containers/:id` | DockerManager.removeContainer | strict | ✅ | ⬜ | ⬜ |

### Docker — Volumes

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 70 | GET | `/api/docker/volumes` | DockerManager.listVolumes | global | ✅ | ⬜ | ⬜ |
| 71 | GET | `/api/docker/volumes/:name` | DockerManager.inspectVolume | global | ✅ | ⬜ | ⬜ |
| 72 | GET | `/api/docker/volumes/:name/export` | DockerManager.exportVolume | strict | ✅ | ⬜ | ⬜ |
| 73 | POST | `/api/docker/volumes/:name/backup` | DockerManager.backupVolumeToStorage | backup | ✅ | ⬜ | ⬜ |
| 74 | DELETE | `/api/docker/volumes/:name` | DockerManager.removeVolume | strict | ✅ | ⬜ | ⬜ |

### Docker — Networks

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 75 | GET | `/api/docker/networks` | DockerManager.listNetworks | global | ✅ | ⬜ | ⬜ |
| 76 | GET | `/api/docker/networks/:id` | DockerManager.inspectNetwork | global | ✅ | ⬜ | ⬜ |
| 77 | POST | `/api/docker/networks` | DockerManager.createNetwork | strict | ✅ | ⬜ | ⬜ |
| 78 | POST | `/api/docker/networks/:id/connect` | DockerManager.connectContainerToNetwork | strict | ✅ | ⬜ | ⬜ |
| 79 | POST | `/api/docker/networks/:id/disconnect` | DockerManager.disconnectContainerFromNetwork | strict | ✅ | ⬜ | ⬜ |

### Docker — Diagnostics e Images

| # | Método | Rota | Controller | Limiter | T | P | V |
|---|---|---|---|---|:-:|:-:|:-:|
| 80 | POST | `/api/docker/diagnostics` | DockerDiagnostics.start | strict | ✅ | ⬜ | ⬜ |
| 81 | GET | `/api/docker/diagnostics/:jobId` | DockerDiagnostics.show | global | ✅ | ⬜ | ⬜ |
| 82 | POST | `/api/docker/images/prune` | DockerManager.pruneImages | strict | ✅ | ⬜ | ⬜ |
| 83 | GET | `/api/docker/images` | DockerManager.listImages | global | ✅ | ⬜ | ⬜ |
| 84 | GET | `/api/docker/images/:id` | DockerManager.inspectImage | global | ✅ | ⬜ | ⬜ |
| 85 | DELETE | `/api/docker/images/:id` | DockerManager.removeImage | strict | ✅ | ⬜ | ⬜ |

### Não-API

| # | Método | Rota | Origem | T | P | V |
|---|---|---|---|:-:|:-:|:-:|
| 88 | GET | `/__transmit/events` | `@adonisjs/transmit` (stream SSE) | ✅ | ⬜ | ⬜ |
| 89 | POST | `/__transmit/subscribe` | `@adonisjs/transmit` | ✅ | ⬜ | ⬜ |
| 90 | POST | `/__transmit/unsubscribe` | `@adonisjs/transmit` | ✅ | ⬜ | ⬜ |
| 91 | GET | `*` | Fallback SPA | ✅ | ⬜ | ⬜ |

> As linhas 14 e 22 contam **dois** pares método+rota cada (`PUT` e `PATCH` no mesmo handler),
> por isso a numeração 1–85 cobre 87 pares. Fonte de verdade: `docs/routes-baseline.txt`.

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
| `@google-cloud/storage` | GCS | `google-cloud-storage` **1.17 — SDK oficial do Google** | ✅ direto |
| — | alternativa unificada S3+GCS+Azure+local | `object_store` 0.14 | 💡 avaliar na Fase 8 |
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
| ~~Criptografia incompatível → credenciais ilegíveis~~ | — | — | ✅ **Eliminado na Fase 3.2**: o Rust descriptografou os ciphertexts reais de produção. IV de 16 bytes tratado |
| ~~GCS sem SDK oficial em Rust~~ | — | — | ✅ **Resolvido na Fase 0.6**: `google-cloud-storage 1.17` é o SDK oficial do Google |
| Migração de dados (D4) perde ou corrompe registros | Média | 🔴 Crítico | Script da Fase 4.9 com teste de round-trip sobre cópia do banco real + snapshot pré-cutover |
| Big-bang (D8) sem rede de proteção | Média | 🔴 Crítico | Fases 12.13 (shadow traffic) e 12.11 (runbook de rollback) são **obrigatórias** |
| Comportamento sutil de restore diverge (filtros) | Alta | 🔴 Crítico | Portar os 3 testes unitários de `restore_filters*` primeiro, como spec |
| Ordem de rotas do Axum difere do Adonis | Alta | 🟡 Médio | Testes explícitos para os 4 pares conflitantes (nota do Apêndice A) |
| Shape do JSON muda e quebra o frontend | Média | 🔴 Crítico | D5 decidido cedo + golden files da Fase 1 + tarefa 12.5 |
| Backend continua evoluindo durante o port | Alta | 🟡 Médio | Feature freeze (0.2) ou CI que roda o contrato contra os dois |
| Escopo dos 47 services subestimado | Média | 🟡 Médio | Reavaliar estimativa ao fim da Fase 6, com dados reais de velocidade |

---

## Como usar este documento

1. ~~Comece pela Fase 0.~~ ✅ Concluída em 2026-08-09. Decisões D1–D10 fechadas na seção 3.
2. Marque os checkboxes conforme conclui. Um item só é marcado quando o critério de "Pronto quando" da fase o cobre.
3. Atualize o Apêndice A (colunas **T**/**P**/**V**) a cada endpoint concluído — é o placar real de paridade.
4. Se uma decisão mudar no meio do caminho, edite a seção 3 registrando **o motivo** — desvio silencioso vira dívida (regra da seção 12 do `AGENTS.md`).

### Artefatos gerados pela Fase 0

| Arquivo | Conteúdo |
|---|---|
| `docs/routes-baseline.txt` | 91 rotas não-HEAD, com os middlewares de cada uma |
| `docs/routes-baseline.json` | mesma fonte, estruturada (para o relatório de cobertura da 1.8) |
| `docs/openapi-baseline.yml` | spec OpenAPI 3.0 do Adonis, 73 paths — alvo do `utoipa` (D10) |
| `docs/schema-baseline.sql` | schema real do SQLite de produção — alvo do migrador (4.9) |
| `docker-compose.test.yml` | MySQL, MariaDB, PostgreSQL, MinIO, SFTP e alvo Docker |
| `tests-fixtures/{mysql,mariadb,postgres}/` | seeds SQL com FK, enum, JSON, view, acentuação e escapes |
| `contract-tests/` | suíte black-box da Fase 1 — harness, matchers, seeds, golden e cobertura |
| `contract-tests/__golden__/` | contratos gravados do Adonis; **versionados**, é o diff deles que denuncia mudança |
| `.github/workflows/contract-tests.yml` | CI da suíte + detecção de golden desatualizado |

### Estado atual

```
Fase 0  ████████████████████  100%   decisões + baselines + ambiente
Fase 1  ████████████████████  100%   harness de contrato
Fase 2  ████████████████████  100%   91/91 rotas · 266 testes · 63 goldens ✅
Fase 3  ████████████████████  100%   fundação back-roco (11/11) ✅
Fase 4  ████████████████████  100%   schema, entidades e migrador de dados ✅
Fase 5  ████████████████████  100%   auth, users, audit, system básico ✅
Fase 6  ██████████████████░░   90%   connections + drivers de banco (9/10)
Fase 7  ░░░░░░░░░░░░░░░░░░░░    0%   backups, dump e restore  ← próxima
Fases 8–12                      0%
```

**22 rotas de `/api` no ar** das 91 do baseline (24%), com **329 testes** em `cargo test`.
A **6.8** é o único item aberto da Fase 6, e foi movido para a Fase 9 porque os quatro resolvers
operam sobre a lista de containers do Docker.

Concluído até aqui: **Fase 0** (exceto 0.2, que depende do time), as **Fases 1 a 5 inteiras** e
**9 dos 10 itens da Fase 6**.
As três primitivas de compatibilidade — **criptografia**, **scrypt** e **token opaco** — foram
validadas contra dados reais de produção, não só fixtures, e o migrador de dados foi conferido
por checksum contra uma cópia do banco de produção (24.608 linhas).

A **Fase 2 rendeu sete achados**, e a Fase 5 rendeu o **oitavo** (`normalizeEmail()`) — o mais
perigoso deles, porque só apareceria depois do cutover, na forma de gente que não consegue mais
entrar sem nenhuma mensagem que explique o porquê.

Números da suíte de contrato:

| | |
|---|---|
| Testes de contrato | **266** (5 pulados por falta de `mysqldump`/`pg_dump`/Docker) |
| Testes do próprio harness | **42** |
| Golden files | **63**, byte-idênticos ao regravar |
| Cobertura de rotas | **91/91 (100%)** |

A especificação executável do back-roco está pronta. **A Fase 2 rendeu sete achados** — de uma
decisão errada no próprio roadmap a um endpoint que devolve HTML com status 200 — todos fixados
por teste e listados na seção da Fase 2.

No lado Rust: **142 testes**, `fmt` e `clippy -D warnings` limpos. A **Fase 4 fechou** — schema
com diff estrutural vazio contra o baseline de produção, 9 entidades geradas, e o migrador de
dados validado contra uma cópia do banco real, preservando byte a byte os hashes de token
(D1), de senha (D2) e os ciphertexts (D3).

Da Fase 3 restam **3.4** (camada de banco do token), **3.5** (limitadores por rota) e **3.8**
(persistência da auditoria) — agora desbloqueados, mas pertencem naturalmente à Fase 5, junto com
as rotas que os consomem. **3.10 saiu do bloqueio e está feito**: os fixtures YAML espelham os
seeds da tarefa 1.5 e carregam com `db seed`.
