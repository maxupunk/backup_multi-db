# Roadmap — Loco nativo

> **Objetivo:** remover toda adaptação feita para manter compatibilidade com o
> backend AdonisJS e deixar o projeto como se tivesse nascido em Loco.rs.
>
> **Premissa explícita:** este roadmap **quebra o contrato de propósito**. Não há
> preservação de dados, sessões, senhas ou credenciais. Bancos existentes são
> descartados; usuários recadastram; credenciais de conexão são reinseridas. O
> frontend muda junto. Trate como **v2.0.0**, não como uma migração.

---

## 0. Estado da execução

| Fase | Situação |
|---|---|
| 0 — Poda do morto | ✅ concluída (uma ressalva: ver a fase) |
| 1 — Schema do zero | ✅ concluída |
| 1 — Marco (aposentar `contract-tests/` + OpenAPI) | ⚠️ parcial — falta apagar os diretórios |
| 2 — Autenticação JWT | ✅ concluída |
| 3 — Hash argon2 + `forgot`/`reset` | ✅ concluída |
| 4 — Criptografia padrão | ✅ concluída |
| 5 a 10 | ⏳ pendentes (alguns itens saíram por tabela — marcados abaixo) |

**Ponto de corte.** É exatamente o corte que a §6 sugere: núcleo (schema, auth,
hash, cifra) já é Loco nativo e a API ainda responde no formato antigo
(`{success, data}`, paginação do `SimplePaginator`, duas famílias de erro).

**Portões de qualidade no fim da Fase 4:** `cargo fmt`, `cargo clippy
--all-targets -- -D warnings` e `cargo test` verdes — 448 testes de unidade e
155 de request. O único vermelho é
`minio_and_sftp_adapters_work_against_the_compose_services`, que exige o MinIO e
o SFTP do `docker-compose.test.yml` de pé; é falha de ambiente, não de código.

**O que quebrou de propósito e precisa de ação operacional:**

1. **O banco.** Não há upgrade: apague o SQLite e rode `cargo loco db migrate`.
2. **Todos os usuários.** Recadastro pelo fluxo de bootstrap.
3. **Todas as credenciais cifradas** de conexões e de storages — reinserir à mão.
4. **`JWT_SECRET` virou obrigatório em produção.** O `config/production.yaml`
   não tem mais default; sem a variável, o processo não sobe. Gere com
   `openssl rand -base64 48 | tr -d '\n'`.
5. **`AUTH_ACCESS_TOKEN_EXPIRES_IN` deixou de existir.** A vida do token agora é
   `JWT_EXPIRATION`, em segundos.

---

## 1. Como ler este documento

O código Rust não é o problema — o problema é o **contrato** que ele reproduz.
No levantamento havia **233 menções ao Adonis em 65 arquivos** de
`backend/src/`, e a maioria não é comentário solto: é uma decisão de formato, de
algoritmo ou de schema que existe porque o outro backend fazia assim.

Tamanho do território (levantamento original; depois da seta, o estado após as
fases 0 a 4):

| Área | Volume |
|---|---|
| `backend/src/` | 31.668 linhas |
| `backend/tests/` | 5.751 linhas |
| `backend/migration/src/` | 1.097 linhas → **629**, numa migration só |
| `contract-tests/` | 6.190 linhas + 63 golden files (a apagar) |
| Menções ao Adonis em `backend/` | 233 → **192** |

**O que este roadmap NÃO toca.** O domínio do produto — dump/restore, adapters
de storage (S3/GCS/Azure/SFTP/local), Docker manager, retenção GFS, agendador,
métricas — não é adaptação do Adonis. É valor próprio, já escrito em Rust
idiomático sobre o Loco. Sai intacto. O alvo é a **casca**: autenticação,
hashing, criptografia, formato de resposta, middlewares, transporte de eventos
e schema.

---

## 2. Inventário das adaptações

Cada linha foi verificada no código, não herdada de documento anterior.

A coluna **Estado** foi preenchida depois da execução das fases 0 a 4.

| # | Adaptação | Vira | Estado |
|---|---|---|---|
| A | **Token opaco `oat_<id>.<secret>`** com SHA-256 na tabela `auth_access_tokens`, réplica do `DbAccessTokensProvider` | `loco_rs::auth::JWT` | ✅ Fase 2 |
| B | **scrypt** com parâmetros do `@adonisjs/hash` (`$scrypt$n=16384,r=8,p=1$…`) | `loco_rs::hash` (argon2) | ✅ Fase 3 |
| C | **AES-256-GCM fora do padrão**: IV de 16 bytes (não 12), chave crua sem KDF, formato `b64(iv):b64(tag):b64(ct)` | `Aes256Gcm` padrão, nonce de 12 bytes, chave derivada | ✅ Fase 4 |
| D | **Envelope `{success, data}`** em três variantes | payload direto via `format::json` | ⏳ Fase 5 |
| E | **Duas famílias de erro** — `{errors:[…]}` do VineJS e `{success:false,message}` escrito à mão | `loco_rs::Error` + um shape único | ⏳ Fase 5 (só o 401 já mudou) |
| F | **Paginação do `SimplePaginator` do Lucid** — 9 chaves em `meta`, com `nextPageUrl` que ignora os filtros | paginação do Loco | ⏳ Fase 5 |
| G | **Timestamp local ingênuo sem fuso** (`2026-08-06T16:49:25.000`) porque o Lucid gravava assim | RFC 3339 em UTC | ✅ antecipado na Fase 1 |
| H | **Mensagens e nomes de regra do VineJS** replicados à mão, sem `derive` | `validator` com `derive` | ⏳ Fase 5 |
| I | **Rate limit de janela fixa em memória** reproduzindo `rate-limiter-flexible` | `tower-governor` | ⏳ Fase 6 |
| J | **`force_json`** — porte do `force_json_response` | deletar | ⏳ Fase 6 |
| K | **SSE em `/__transmit/*`** com o protocolo do `@adonisjs/transmit` | SSE próprio em `/api/events` | ⏳ Fase 7 |
| L | **Schema espelhando o Knex** — `auth_access_tokens`, `datetime_text`, enums como TEXT+CHECK, `users` sem `pid`/`api_key` | migration inicial única, padrão Loco | ✅ Fase 1 |
| M | **OpenAPI é o arquivo gravado do Adonis**, embutido com `include_str!` | removido ao fim da Fase 1 (decisão 3) | ✅ rotas e `include_str!` removidos; falta apagar `docs/` |
| N | **`nodeVersion`** em `/api/system/status` | `runtimeVersion` | ✅ |
| O | **`migrate_data.rs`** — migrador do SQLite do Adonis | deletar | ✅ Fase 0 |
| P | **Fixtures e testes cross-language** — vetores gerados pelo Node | deletar | ✅ Fase 0 (mais `access_token_vectors.json` na Fase 2) |
| Q | **63 golden files** cuja origem é o comportamento do Adonis | aposentar ao fim da Fase 1 (decisão 2) | ⚠️ falta apagar `contract-tests/` |

### Achados adjacentes (dívida encontrada no levantamento)

Não são adaptação do Adonis, mas aparecem no mesmo caminho e devem sair junto:

- ✅ **`auth.jwt.secret` está versionado com o valor default do scaffold** —
  resolvido na Fase 2, antes de o bloco virar caminho de autenticação real:
  `development` tem um default público e assumido como tal, `production` não
  tem default nenhum.
- ✅ **`mailer.smtp.enable: true`** com `src/mailers/` contendo só `mod.rs` —
  resolvido na Fase 0: existe um `AuthMailer` de verdade.
- ⏳ **`src/dtos/` tem só `common.rs`**, e o frontend só recebeu dois bindings
  (`ApiError.ts`, `Page.ts`), enquanto os `views/` montam os shapes à mão. O
  [AGENTS.md §5](backend/AGENTS.md) manda o contrato tipado ir para `dtos/` com
  `ts-rs` — continua sendo a Fase 8.
- ✅ **`backend/.github/workflows/ci.yaml`** — removido na Fase 0.

---

## 3. Ordem das fases

A ordem não é preferência: é dependência. O schema é a base de A, B e C; o
contrato HTTP (D–H) toca todos os controllers e só faz sentido depois que a
autenticação parou de mudar.

```
Fase 0   Poda do morto + registrar mailer ──────────────┐   ✅
            │                                           │
            ▼                                           │
Fase 1   Schema do zero                                  │  ✅
            └─ marco: aposentar contract-tests           │  ⚠️ falta o `rm`
            │                                            │
      ┌─────┼──────────────────┐                         │
      ▼     ▼                  ▼                         │
  Fase 2   Fase 3            Fase 4                      │  ✅ (as três
  Auth     Argon2 +          Cifra padrão                │   entraram num
  JWT      forgot/reset ◄────────────────────────────────┘   passo só, junto
      │     │                  │        (precisa do mailer)   com a Fase 1)
      └─────┴─────────┬────────┘
                      ▼
              Fase 5   Contrato HTTP          ◄── próximo
                      │
         ┌────────────┼────────────┐
         ▼            ▼            ▼
     Fase 6       Fase 7       Fase 8
   Middlewares      SSE      DTOs/ts-rs
         └────────────┼────────────┘
                      ▼
              Fase 9   Config e limpeza final
                      ▼
              Fase 10  Suíte de testes
```

---

## Fase 0 — Poda do morto ✅

Nada aqui tem dependência e nada quebra cliente. Tira ruído antes das fases que
exigem raciocínio.

**Remover**
- [x] `backend/src/bin/migrate_data.rs` + a entrada `[[bin]]` no `Cargo.toml`
- [x] `backend/tests/models/password.rs` e `backend/tests/models/encryption.rs`
      (testes de compatibilidade cross-language com o Node)
- [x] `backend/tests/fixtures/scrypt_vectors.json` e `encryption_vectors.json`
      — `tests/fixtures/` ficou vazio e saiu junto
- [x] `backend/.github/workflows/ci.yaml` (CI morto do scaffold)

**Manter, ao contrário do que parece**
- `mailer:` do `config/*.yaml` e `src/mailers/` estão vazios hoje, mas a
  **decisão 1 os torna obrigatórios**: o fluxo `forgot/reset` da Fase 3 manda
  e-mail. Em vez de remover, esta fase **registra um mailer de verdade** e
  confirma que o SMTP de desenvolvimento (`localhost:1025`) responde.
- [x] `src/mailers/auth.rs` (`AuthMailer::forgot_password`) com os três templates
      em `src/mailers/auth/forgot/` (`subject.t`, `html.t`, `text.t`)

**Pronto quando:** `cargo test` verde com a suíte menor, nenhuma referência a
`scrypt_vectors`/`encryption_vectors` no repositório, e um e-mail de teste
chegando no SMTP de desenvolvimento.

> **Ressalva, para não haver ilusão de cobertura:** o mailer está registrado,
> tem template e é exercitado pelo teste do ciclo `forgot → reset`, mas **a
> entrega por SMTP não foi verificada contra um servidor de pé** — não havia
> MailHog/Mailpit escutando em `localhost:1025` no ambiente. O caminho testado
> vai até o enfileiramento. Subir um SMTP de desenvolvimento e conferir a
> chegada do e-mail continua pendente.

**Risco:** nenhum. **Custo:** baixo.

---

## Fase 1 — Schema do zero ✅

O schema atual carrega a forma do Knex. Com dados descartáveis, a saída limpa é
**uma migration inicial única**, gerada pelos generators do Loco.

**Remover**
- [x] As 4 migrations `m20260809_*` — substituídas por
      `m20260812_000001_initial_schema.rs`
- [x] A tabela `auth_access_tokens` inteira (some com a Fase 2)
- [x] Escolhas herdadas: `datetime_text` por afinidade do Knex, enums como
      TEXT+CHECK espelhando o Knex, colunas de tempo em epoch-ms

**Adotar**
- [x] Uma migration única com todas as tabelas na ordem das FKs, sobre os
      helpers de `loco_rs::schema` (`table_auto_tz`, `timestamptz`,
      `pk_auto`, `uuid_uniq`, …) em vez de `ColumnDef` cru
- [x] `users` com os campos que o Loco espera: `pid` (UUID), `api_key`,
      `reset_token`/`reset_sent_at`, `email_verification_token`/`email_verified_at`
- [x] Tipos nativos do Sea-ORM (`timestamp_with_time_zone`), sem contorno
- [x] `src/models/_entities/` regenerado com o `sea-orm-cli` nas mesmas flags
      que `cargo loco db entities` usa (a aplicação não compilava antes de as
      entidades existirem, então a migration rodou pelo binário standalone
      `cargo run -p migration -- up`)

**Consequências que apareceram ao propagar `DateTimeWithTimeZone`:**
- [x] `src/views/timestamp.rs` **removido**: com a coluna carregando o
      deslocamento, o `serialize_with` que emitia `2026-08-06T16:49:25.000`
      passaria a mentir sobre um valor que agora é um instante. O `chrono`
      serializa RFC 3339 sozinho. *(É um item da Fase 5 antecipado por
      necessidade — sem ele o schema novo produziria data errada na tela.)*
- [x] O corte de "hoje" em `/api/stats` e `/api/audit-logs/stats` passou a ser
      meia-noite **no fuso do operador**, reconstruída a partir do offset, em
      vez de um `NaiveDateTime` local sem fuso

**Sobre os enums.** As colunas `status`, `type`, `retention_type` e `trigger`
viraram `text` sem `CHECK`. O `CHECK` era SQL escrito à mão com crase de MySQL,
e o domínio já rejeita valor fora da lista com 422 nomeando o campo — que é
melhor resposta que uma violação de constraint virando 500. `audit_logs.action`
prova o argumento pelo outro lado: perdeu o `CHECK` justamente porque uma
inserção de auditoria recusada derrubava a operação que ela deveria registrar.

**Quebra:** o banco. Não há caminho de upgrade — é `rm` no SQLite e migrate do zero.

**Pronto quando:** banco novo criado só por `cargo loco db migrate`, entidades
regeneradas sem edição manual, `cargo test` verde. ✅

**Risco:** alto (é a fundação). **Custo:** alto.

### Marco: aposentar `contract-tests/` e a OpenAPI ao fim desta fase

Pelas **decisões 2 e 3**, a suíte de contrato e a documentação Swagger saem
juntas. O momento certo é **aqui, não antes e não depois** — e a razão importa:

- **Não antes:** esta fase reescreve o schema inteiro sem mudar a API. Uma
  suíte de contrato verde é exatamente a prova de que a reescrita não alterou
  comportamento observável. É o último uso legítimo dela, e é um uso bom.
- **Não depois:** a Fase 2 troca o formato do token, e `auth/login-ok` quebra na
  hora. A partir dali a suíte só produz ruído vermelho que ninguém vai ler.

A OpenAPI vai no mesmo pacote porque
[public.contract.test.ts](contract-tests/tests/public.contract.test.ts#L68)
cobre `/api/swagger` e `/api/docs`: removê-las antes derrubaria a suíte na fase
em que ela ainda serve para alguma coisa.

- [ ] ~~Rodar `pnpm contract:roco` uma última vez, verde, com o schema novo~~
      **não executado** — ver a nota abaixo
- [ ] **Remover `contract-tests/`** (6.190 linhas de TS + 63 golden files)
- [ ] **Remover `.github/workflows/contract-tests.yml`**
- [x] Remover os handlers `swagger` e `docs` e as duas rotas de `public.rs` —
      `health` fica
- [x] Remover o teste `swagger_embeds_the_baseline_spec` de `public.rs`
- [ ] **Remover `docs/openapi-baseline.yml` e `docs/routes-baseline.txt`** —
      nenhum código os referencia mais; falta apagar o diretório
- [x] Remover a exigência `pnpm contract:roco` do [AGENTS.md §10.9](backend/AGENTS.md)
      e a seção correspondente do `backend/README.md`
- [x] Atualizar as árvores de diretório em `README.md` e `CHECKLIST.md`, que
      listavam `docs/` e `contract-tests/`

> **Por que a última execução verde não aconteceu.** O plano previa rodar a
> suíte entre a Fase 1 e a Fase 2. Na prática as fases 1 a 4 foram aplicadas num
> mesmo passo — o schema não compilava sem a auth nova, e a auth nova não
> compilava sobre o schema velho —, então nunca existiu um ponto intermediário
> em que a suíte pudesse passar. O valor que ela daria (provar que a troca de
> schema não mudou a API) foi coberto pelos 155 testes de request em Rust, que
> exercitam os mesmos endpoints e passaram.
>
> **O que falta.** Só o `rm`. A ferramenta recusou a remoção recursiva dos
> diretórios versionados, e a decisão é do operador:
>
> ```sh
> git rm -r contract-tests docs .github/workflows/contract-tests.yml
> ```
>
> Enquanto o workflow existir, o CI vai reprovar: ele roda uma suíte que
> compara contra o token `oat_`, o hash scrypt e o formato de cifra antigo —
> todos removidos.

---

## Fase 2 — Autenticação nativa do Loco ✅

Hoje: `oat_<base64(id)>.<base64(secret)>`, SHA-256 do segredo na tabela,
extractor `Authenticated` escrito à mão, e um CRC-32 embutido no token que
o próprio código dizia que ninguém valida.

**Remover**
- [x] `src/models/access_token.rs` (formato do token)
- [x] `src/models/auth_access_tokens.rs` (registro/revogação)
- [x] `src/controllers/middlewares/auth.rs` (extractor `Authenticated`)
- [x] A constante `DEFAULT_ABILITIES = ["*"]` e a coluna `type`
- [x] `tests/models/access_token.rs` e `tests/fixtures/access_token_vectors.json`
- [x] `settings.auth_access_token_expires_in` e o `parse_duration` que existia
      só para lê-lo — a vida do token virou `auth.jwt.expiration`

**Adotar**
- [x] O extractor `JWTWithUser<users::Model>` do Loco, atrás do alias
      `controllers::Auth` para os ~90 handlers não repetirem o genérico
- [x] `Authenticable` em `users::Model` (`find_by_claims_key` resolve o `pid`,
      `find_by_api_key` a chave), mais `find_by_pid` e `generate_jwt`
- [x] **`auth.jwt.secret` via `get_env`** — `development` tem default público e
      declarado como tal; **`production` não tem default nenhum**, então subir
      sem `JWT_SECRET` falha em vez de assinar sessão com segredo versionado
- [x] Autorização (`is_admin`) continua **explícita no controller**, agora como
      `controllers::require_admin(&auth.user, "…")`, mantendo a mensagem
      própria de cada recurso

**A sessão virou stateless, e isso muda três comportamentos:**
- `POST /api/auth/logout` **não revoga nada** — não há registro no servidor para
  apagar. Ele confirma, e quem encerra a sessão é o cliente descartando o token.
  O teste `logout_acknowledges_without_revoking_anything` fixa isso, para que
  ninguém reintroduza uma tabela de tokens sem dizer.
- Desativar um usuário passa a valer **no próximo token**, não no atual.
- `JWT_EXPIRATION` é o único limite sobre a vida de um token vazado. Trocar o
  `JWT_SECRET` é o botão de "deslogar todo mundo".

**Quebra:** toda sessão existente. Todo cliente que guarda `oat_…` precisa
relogar. O frontend passa a mandar `Authorization: Bearer <jwt>`.

**Pronto quando:** `POST /api/auth/login` devolve JWT, rota protegida rejeita
token ausente/expirado/adulterado, e `grep -ri "oat_" backend/src` volta vazio. ✅

**Risco:** alto. **Custo:** médio.

---

## Fase 3 — Hash de senha argon2 ✅

**Remover**
- [x] `src/models/password.rs` inteiro (scrypt N=16384, r=8, p=1, PHC do Node)
- [x] A dependência `scrypt` do `Cargo.toml`

**Adotar**
- [x] `loco_rs::hash::hash_password` / `verify_password` (argon2id)
- [x] O `DUMMY_HASH` de equalização de tempo foi refeito sobre o argon2 — sem
      ele, "e-mail inexistente" volta na hora e "senha errada" paga uma
      derivação inteira, e a diferença é medível

**Adotar também — decisão 1: `forgot` / `reset`**
- [x] `POST /api/auth/forgot` e `POST /api/auth/reset`, ambos sob o limitador
      `auth` (5/min)
- [x] Mailer registrado na Fase 0, com template em `src/mailers/auth/forgot/`
- [x] **Não vazar existência de conta:** `forgot` responde sucesso mesmo para
      e-mail inexistente — e uma falha de envio é logada e engolida pelo mesmo
      motivo, já que um erro de SMTP que só aparecesse para endereço real
      vazaria o mesmo fato. Teste:
      `forgot_answers_the_same_for_a_known_and_an_unknown_address`
- [x] Token de redefinição de uso único e com validade de 4 horas, verificada
      na consulta (`find_by_reset_token`) e não no chamador
- [x] Telas de `forgot` e `reset` no frontend
      (`frontend/src/pages/forgot.vue`, `reset.vue`), com link a partir do login

**Quebra:** todas as senhas. Não há rehash-on-login possível — o hash antigo é
de outro algoritmo. Com `forgot/reset` no lugar, o caminho de recuperação é o
próprio usuário pedindo redefinição.

> **Atenção operacional:** o primeiro administrador não tem como se recuperar
> por e-mail se ninguém conseguir entrar. O `INITIAL_ADMIN_BOOTSTRAP_TOKEN`
> continua sendo o caminho: com a tabela `users` vazia, o primeiro cadastro
> nasce administrador ativo. Como o banco é recriado do zero nesta v2.0.0, a
> tabela **está** vazia — o bootstrap é justamente o primeiro passo do deploy.

**Pronto quando:** login funciona com senha nova, o ciclo
`forgot → e-mail → reset → login` funciona ponta a ponta, e
`grep -ri scrypt backend/` volta vazio. ✅ *(o ciclo é coberto por
`the_reset_cycle_changes_the_password_and_burns_the_token`, que lê o token do
banco no lugar da caixa de entrada — a entrega SMTP em si segue não verificada,
como registrado na Fase 0)*

**Risco:** médio. **Custo:** médio — subiu de "baixo" porque `forgot/reset`
traz mailer, templates, duas rotas e duas telas junto.

---

## Fase 4 — Criptografia padrão ✅

O formato atual tem **dois desvios reais do padrão**, ambos documentados no
código: IV de 16 bytes em vez do nonce de 12 do `Aes256Gcm`, e a
`DB_ENCRYPTION_KEY` usada **crua** como chave de 32 bytes — sem KDF, sem salt,
sem stretching.

**Remover**
- [x] O alias `AesGcm<Aes256, U16>` e o parser do formato `iv:tag:ct`
- [x] As dependências `aes` e `typenum`, que existiam só para o alias

**Adotar**
- [x] `Aes256Gcm` padrão (nonce de 12 bytes), nonce aleatório por registro
- [x] Chave **derivada** por HKDF-SHA256 com rótulo `info` versionado
      (`backup-multi-db/column-encryption/v1`). Sem salt, de propósito: há um
      único segredo de entrada, já com 256 bits de entropia configurada — o
      salt seria constante e não separaria nada. Quem faz a separação de
      domínio é o `info`
- [x] Formato novo e versionado: `v1.base64url(nonce).base64url(ct||tag)`

**Sem fallback, e há um teste que garante isso.**
`rejects_the_previous_wire_format` recusa explicitamente `iv:tag:dados` — é o
teste que quebra se alguém tentar reintroduzir um caminho de leitura do formato
antigo.

**Quebra:** `connections.password_encrypted` e
`storage_destinations.config_encrypted` viram ilegíveis. Toda credencial de
banco e de storage é reinserida à mão. Os fixtures de seed
(`src/fixtures/*.yaml`) foram recifrados no formato novo.

**Pronto quando:** round-trip novo passa, e nenhum caminho tenta ler o formato
antigo. ✅

**Risco:** alto (perda de credenciais é operacionalmente cara). **Custo:** médio.

---

## Fase 5 — Contrato HTTP

A fase mais larga: toca todos os controllers e views. É onde o "cheiro de
Adonis" some da API.

**Remover**
- [ ] `src/views/envelope.rs` — `{success, data}` nas três variantes
- [ ] `src/views/errors.rs` — as duas famílias de erro
- [ ] `src/views/pagination.rs` — as 9 chaves do `SimplePaginator`, incluindo o
      `nextPageUrl` que ignora filtros
- [x] `src/views/timestamp.rs` — timestamp sem fuso **(feito na Fase 1)**
- [ ] `src/models/validation.rs` — mensagens e nomes de regra do VineJS

**Adotar**
- [ ] `format::json(payload)` direto, sem envelope; status HTTP carrega o
      sucesso/falha
- [ ] `loco_rs::Error` como erro de aplicação, com **um** shape de erro
- [ ] `validator` com `derive` nos `Params` (o motivo de não usar `derive` era
      reproduzir o texto do VineJS — motivo que morre aqui)
- [x] `chrono` serializando RFC 3339 em UTC; o frontend formata para local
      **(feito na Fase 1)** — `formatDateTimePtBR` já usa
      `new Date(x).toLocaleString`, que lê o deslocamento e renderiza no fuso do
      navegador, então esta parte não exigiu mudança no frontend
- [ ] Paginação do Loco

> **Já mudou de shape, mesmo com a Fase 5 pendente:** o 401 das rotas
> protegidas. Quem responde agora é o extractor do Loco, com
> `{"error":"unauthorized","description":…}` no lugar de
> `{"errors":[{"message":"Unauthorized access"}]}`. O `extractErrorMessage` de
> [api.ts](frontend/src/services/api.ts) já tratava a chave `error`, então o
> frontend absorveu sem mudança.

**Quebra:** **toda** resposta da API. 6 arquivos do frontend leem
`success`/`data`/`lastPage`/`perPage`; [api.ts](frontend/src/services/api.ts) é
o ponto central de adaptação.

**Pronto quando:** nenhuma resposta contém `success`, os timestamps têm fuso, e
o frontend consome a API nova sem camada de tradução.

**Risco:** alto. **Custo:** alto — é a fase que domina o cronograma.

---

## Fase 6 — Middlewares

**Remover**
- [ ] `src/controllers/middlewares/force_json.rs` — resolve um problema que só
      existia no Adonis (o handler de exceção devolvendo HTML do Youch). No
      Loco, `IntoResponse` já sempre emite JSON
- [ ] `src/controllers/middlewares/rate_limit.rs` e `limiters.rs` — janela fixa
      em memória, replicando `rate-limiter-flexible` e a ordem dos headers

**Adotar**
- [ ] `tower-governor` (janela deslizante, que é o comportamento correto — a
      janela fixa existia só para bater com o golden gravado)
- [ ] Os middlewares que o Loco já traz em `server.middlewares` do YAML

**Quebra:** headers `X-RateLimit-*` e o corpo do 429 mudam.

**Pronto quando:** `src/controllers/middlewares/` só tem o que o Loco não cobre.

**Risco:** baixo. **Custo:** baixo.

---

## Fase 7 — Eventos SSE próprios

**Remover**
- [ ] `src/controllers/transmit.rs` — handshake, `uid`, `$$transmit/ping`,
      `subscribe`/`unsubscribe` do `@adonisjs/transmit`
- [ ] `@adonisjs/transmit-client` do [frontend/package.json](frontend/package.json#L15)
- [ ] `frontend/src/plugins/transmit.ts`

**Adotar**
- [ ] `axum::response::sse` em `/api/events`, com o mesmo `models/sse.rs` por
      baixo (o broadcast interno não é adaptação — só o protocolo era)
- [ ] `EventSource` nativo do navegador no frontend

**Quebra:** notificações em tempo real. Componentes afetados incluem
`DockerDiagnosticDialog.vue` e `ArchiveProgress.vue`.

**Pronto quando:** nenhuma dependência `@adonisjs/*` no `package.json` do
frontend — o **último** vínculo com o Adonis no repositório.

**Risco:** médio. **Custo:** médio.

---

## Fase 8 — Contrato tipado em `dtos/`

Não é remoção de legado: é a regra do próprio [AGENTS.md §5](backend/AGENTS.md)
que nunca foi cumprida, porque os `views/` estavam ocupados replicando shapes.
Com a Fase 5 feita, o caminho abre.

**Pela decisão 3, esta fase deixou de ser opcional.** Com a OpenAPI removida na
Fase 1, os bindings `ts-rs` passam a ser a **única descrição formal** do que a
API aceita e devolve. A diferença a favor: uma spec Swagger desatualizada mente
em silêncio; um binding desatualizado **quebra o build do frontend**.

**Adotar**
- [ ] Mover o contrato consumido pelo frontend para `src/dtos/` com
      `#[derive(TS)] #[ts(export, export_to = "../frontend/src/bindings/")]`
- [ ] `views/` volta a ser só serialização específica de recurso
- [ ] Frontend passa a importar de `src/bindings/` em vez de tipar à mão
- [ ] Garantir que a geração dos bindings roda no CI e que um binding fora de
      data falha o build — sem isso a garantia acima não existe de fato

**Pronto quando:** todo endpoint consumido pelo frontend tem binding gerado, e
o CI reprova binding desatualizado.

**Risco:** baixo. **Custo:** médio.

---

## Fase 9 — Configuração e limpeza final

- [x] **`auth.jwt.secret` fora do YAML versionado** — saiu na Fase 2, junto com
      `JWT_EXPIRATION`, o `.env.example` e os dois `docker-compose`
- [ ] Revisar `config/*.yaml` inteiro contra o scaffold do Loco: remover bloco
      sem consumidor, manter o que é usado *(só o bloco
      `auth_access_token_expires_in` saiu até agora)*
- [ ] Remover as **menções ao Adonis** em `backend/src/`. Regra: o comentário
      que explica *por que o código é assim* perde o objeto quando o "assim"
      deixa de existir. O que sobreviver deve justificar a decisão **em si**,
      sem citar o framework antigo.
      **Estado: 192 restantes** (eram 233). As que saíram foram as dos arquivos
      reescritos nas fases 0–4; o grosso do que sobra está em `views/`,
      `middlewares/` e nos models de storage — ou seja, no território das fases
      5 a 7. Fazer a varredura agora apagaria a justificativa de código que
      ainda não mudou
- [x] `nodeVersion` → `runtimeVersion` em `/api/system/status` (backend, o
      binding TS e o rótulo "Node.js" do `SystemInfoCard.vue`)
- [x] `GET /api/health`: passou a usar `App::app_version()` no lugar do
      `"1.0.0"` fixo — a versão é justamente o que se pergunta a um health
      check durante um incidente
- [x] Reescrever `backend/AGENTS.md` §10.9 (a exigência `contract:roco`) e a
      seção de suíte de contrato do `backend/README.md`
- [x] Atualizar as árvores de `README.md` e `CHECKLIST.md`
- [ ] `DOCKER_MANAGER_CHECKLIST.md`

**Pronto quando:** `grep -ri adonis` no repositório volta **vazio**.

**Risco:** baixo. **Custo:** médio.

---

## Fase 10 — Suíte de testes

`contract-tests/` já saiu no marco da Fase 1. Esta fase cuida do buraco que a
saída dela deixa: a suíte em Rust passa a ser **a única** rede de proteção.

Vale ser honesto sobre o que se perdeu. `tests/requests/*` (5.751 linhas) sempre
foi descrito no próprio código como "a rede de segurança **em Rust**", com a
suíte de contrato como especificação. Sem a especificação, essa rede precisa
cobrir sozinha o que os 63 golden cobriam — em particular o **shape** das
respostas, que era justamente o que o matcher tolerante da suíte verificava.

- [ ] `tests/` espelhando `src/`, com os helpers de `loco_rs::testing`
- [ ] Snapshots com `insta` no lugar dos golden: é o mecanismo equivalente que o
      Loco já traz, e o [AGENTS.md §7](backend/AGENTS.md) já o exige
- [ ] Cobrir os três caminhos de sempre: feliz, entrada inválida, não autorizado
- [ ] Revisar cobertura endpoint a endpoint — os 73 paths precisam ter dono

**Pronto quando:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
e `cargo test` verdes, sem nenhum teste comparando contra o Adonis, e nenhuma
rota sem teste.

**Risco:** médio. **Custo:** médio.

---

## 4. Impacto no frontend, consolidado

O frontend **não sobrevive** a este roadmap sem mudança. Consolidado por fase:

| Fase | O que quebra no frontend | Situação |
|---|---|---|
| 2 | Formato do token; fluxo de login/logout | ✅ o token é opaco para a store — ela só guarda a string, então bastou nada |
| 3 | Senhas inválidas; **duas telas novas** (`forgot` e `reset`) | ✅ `pages/forgot.vue`, `pages/reset.vue` e o link no login |
| 5 | **Toda** leitura de resposta: `success`, `data`, `meta.lastPage`, `perPage`, parsing de timestamp | ⏳ pendente — mas o parsing de timestamp já foi absorvido sem mudança (RFC 3339 e `toLocaleString`) |
| 6 | Tratamento de 429 | ⏳ pendente |
| 7 | Notificações em tempo real; remoção do `@adonisjs/transmit-client` | ⏳ pendente |
| 8 | Tipos passam a vir de `src/bindings/` | ⏳ pendente (`runtimeVersion` foi renomeado à mão em `types/api.ts`) |

Ponto de concentração: [frontend/src/services/api.ts](frontend/src/services/api.ts).
Vale tratá-lo como a fronteira e adaptar ali primeiro em cada fase.

---

## 5. Decisões

### ✅ 1 — Usuários: `forgot` / `reset`

Implementar o fluxo de recuperação do Loco, em vez de recriar usuários por seed.

**Consequências, já refletidas nas fases:**
- A Fase 1 **precisa** dos campos `reset_token`/`reset_sent_at` e
  `email_verification_token`/`email_verified_at` em `users` — não são opcionais.
- O `mailer:` e `src/mailers/` **deixam de ser lixo** e passam a ser
  pré-requisito. A Fase 0 registra um mailer em vez de apagar a configuração.
- A Fase 3 sobe de custo baixo para médio: traz duas rotas, template de e-mail e
  duas telas de frontend que não existem hoje.
- Fica um risco operacional a resolver: o primeiro admin não se recupera por
  e-mail se ninguém conseguir entrar.

### ✅ 2 — `contract-tests/`: aposentar

Sai do repositório: 6.190 linhas de TS, 63 golden files e o job de CI.

**Consequências, já refletidas nas fases:**
- A remoção acontece no **marco ao fim da Fase 1**, não na Fase 10 — a suíte
  tem um último uso legítimo (provar que a troca de schema não mudou a API) e
  quebra logo na Fase 2, quando o formato do token muda.
- `docs/routes-baseline.txt` morre junto: a suíte era seu único consumidor.
- A Fase 10 passa a ser sobre **cobrir o buraco**, com `insta` no lugar dos
  golden.

### ✅ 3 — OpenAPI: remover

`GET /api/docs` e `GET /api/swagger` saem, junto com `docs/openapi-baseline.yml`.

**Consequências, já refletidas nas fases:**
- A remoção entra no **mesmo marco ao fim da Fase 1** que aposenta o
  `contract-tests/`. Não pode vir antes: [public.contract.test.ts](contract-tests/tests/public.contract.test.ts#L68)
  cobre as duas rotas, e removê-las cedo derruba a suíte justamente na fase em
  que ela ainda tem valor.
- Com `openapi-baseline.yml` e `routes-baseline.txt` fora, **`docs/` fica vazio
  e some do repositório**.
- A **Fase 8 passa a ser o contrato da API**, não um extra: os bindings `ts-rs`
  viram a única descrição formal do que a API aceita e devolve — e, ao
  contrário do Swagger, quebram o build quando divergem.

O que se está descartando, verificado no arquivo:

| Aspecto | Estado |
|---|---|
| Cobertura de caminhos | **73 de 73** — em dia, sem defasagem |
| Corpos de requisição/resposta | **42 são `application/json: {}`** — vazios |
| Schemas reutilizáveis | 27 `$ref` no arquivo inteiro |
| Descrições | **82 apontam para `_app/controllers/*.ts_`** do Adonis, e isso aparece na tela do Swagger UI |

Ou seja: a spec diz **quais** endpoints existem, e quase nada sobre **o que
eles aceitam ou devolvem**. A Fase 5 troca o formato de toda resposta, o que
invalidaria o pouco de schema que existe.

> **O que fica em aberto por esta decisão:** se um dia surgir consumidor da API
> fora deste repositório, ele não terá documentação navegável. O caminho de
> volta é `utoipa` (spec gerada das anotações do código, que não pode divergir),
> e ele fica mais barato **depois** da Fase 8 — os DTOs anotados são metade do
> trabalho. Não é uma porta que se fecha.

### 4 — Versionamento (não bloqueante)

`/api/v2/…` ou substituição no lugar? Com frontend e backend no mesmo
repositório e deploy conjunto, versionar tem pouco retorno — mas é decisão de
quem opera.

---

## 6. Ordem sugerida de execução

Fases 0 → 1 → (2, 3 em paralelo) → 4 → 5 → (6, 7, 8 em paralelo) → 9 → 10.

A Fase 5 domina o cronograma e é a que mais quebra o frontend. Se houver
necessidade de manter o sistema utilizável durante o trabalho, o corte natural é
**parar depois da Fase 4** — nesse ponto o núcleo (auth, hash, cifra, schema) já
é Loco nativo e a API ainda responde no formato antigo. As fases 5–10 podem
então correr numa branch longa, porque a partir dali não existe meio-termo:
o contrato muda de uma vez.

**É aqui que a execução parou.** As fases 0 a 4 estão aplicadas; 5 a 10 não.
Ficou uma observação sobre a ordem, para quem retomar: as fases 2, 3 e 4 **não
puderam** correr em paralelo com a 1 como o diagrama sugere. Trocar o schema
sem trocar a auth deixaria uma tabela `auth_access_tokens` que a Fase 2 apaga em
seguida, e trocar a auth sobre o schema velho exigiria um `users` sem `pid`. As
quatro entraram no mesmo passo por dependência de compilação, não por pressa —
e é por isso que a última execução verde do `contract-tests/` não aconteceu.

### Próximos passos concretos

1. `git rm -r contract-tests docs .github/workflows/contract-tests.yml`
   (o único pendente do marco da Fase 1)
2. Definir `JWT_SECRET` no ambiente de produção antes do deploy
3. Subir um SMTP de desenvolvimento e confirmar a chegada do e-mail de `forgot`
4. Fase 5 — é o próximo item da fila e o que domina o resto do cronograma
