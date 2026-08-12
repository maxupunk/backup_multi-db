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
| 5 — Contrato HTTP | ✅ concluída (backend **e** frontend) |
| 6 — Middlewares | ✅ concluída |
| 7 — Eventos SSE próprios | ✅ concluída — **nenhum `@adonisjs/*` no repositório** |
| 8 — Contrato tipado em `dtos/` | ⚠️ backend concluído — **`views/` deixou de existir**, 53 bindings gerados; falta o frontend consumi-los |
| 9 — Configuração e limpeza final | ⚠️ parcial — 136 → **119** menções ao Adonis, em 40 arquivos |
| 10 — Suíte de testes | ⚠️ parcial — suíte **100% verde**, inclusive o teste de MinIO/SFTP; falta a auditoria de cobertura rota a rota |

**Portões de qualidade agora:** `cargo fmt`, `cargo clippy --all-targets -- -D
warnings` e `cargo test` verdes — **458 testes de unidade e 156 de request, 0
falhas** —, mais `npm run type-check` e `npm run build` do frontend.

**Não há mais nenhum teste vermelho.** O
`minio_and_sftp_adapters_work_against_the_compose_services` passa desde que o
`docker-compose.test.yml` esteja de pé:

```sh
docker compose -f docker-compose.test.yml up -d minio minio-init sftp
cd backend && cargo test
```

O `minio-init` cria os buckets `backups-primary`, `backups-secondary` e
`archives` e encerra. O `ci.yml` continua excluindo o teste porque o runner não
sobe os serviços; excluir por ambiente é diferente de aceitar um vermelho.

**O que quebrou de propósito e precisa de ação operacional:**

1. **O banco.** Não há upgrade: apague o SQLite e rode `cargo loco db migrate`.
2. **Todos os usuários.** Recadastro pelo fluxo de bootstrap.
3. **Todas as credenciais cifradas** de conexões e de storages — reinserir à mão.
4. **`JWT_SECRET` virou obrigatório em produção.** O `config/production.yaml`
   não tem mais default; sem a variável, o processo não sobe. Gere com
   `openssl rand -base64 48 | tr -d '\n'`.
5. **`AUTH_ACCESS_TOKEN_EXPIRES_IN` deixou de existir.** A vida do token agora é
   `JWT_EXPIRATION`, em segundos.
6. **Toda resposta da API mudou de forma.** Sem envelope, sem `success`, uma só
   família de erro, paginação em `{results, pagination}`. Qualquer cliente que
   não seja este frontend precisa ser reescrito — ver a Fase 5.
7. **`?limit=` virou `?page_size=`** nas listagens. O `?limit=` do
   `GET /api/storages/:id/browse` **não** mudou: é o teto de objetos listados,
   não paginação.
8. **O limitador de autenticação conta por IP**, não mais por IP+e-mail, e por
   isso subiu de 5 para 20 por minuto — ver a Fase 6.
9. **O SSE trocou de endereço e de protocolo:** `GET /api/events?channels=…` no
   lugar de `/__transmit/*` — ver a Fase 7.
10. **Os booleanos viraram booleanos.** `scheduleEnabled`, `enabled`,
    `compressed` e `protected` saíam como `0`/`1` porque o Lucid entregava o
    inteiro cru do SQLite. Agora são `true`/`false`. Qualquer cliente que
    compare com `=== 1` para de funcionar — ver a Fase 8.
11. **`connection` é sempre chave em um backup**, valendo `null` quando o
    registro ficou órfão. Antes a chave sumia nas rotas sem preload.
12. **`fileSize` e `durationSeconds` de `POST /api/connections/:id/backup` saem
    em bytes e segundos**, não mais como texto formatado (`1.50 MB`). Formatar é
    decisão de apresentação, e o mesmo nome de campo não pode ser número numa
    resposta e texto noutra.
13. **`GET /api/auth/me` e o usuário do `login`/`register` passaram a devolver o
    mesmo `User`** de `/api/users` — ganharam `updatedAt`, e o do login ganhou
    `createdAt`.
14. **`actionDescription`, `actionIcon` e `statusColor` da auditoria** agora
    valem `null` para um valor desconhecido, em vez de a chave sumir.
15. **`replicas` de `browse` é sempre uma lista**, vazia quando o arquivo só
    existe num destino.

### Dois defeitos encontrados durante a execução

Nenhum dos dois é consequência do roadmap; os dois foram corrigidos.

- **`SSE registry was not initialized` no primeiro tick de métricas.** O
  `run_app` do Loco executa `before_run` → `initializers[].before_run` →
  `routes`, e os registros do `shared_store` moravam em `routes`. O
  inicializador de métricas subia antes deles existirem. Os registros passaram
  para `Hooks::before_run`, que é onde estado compartilhado pertence — uma
  função de roteamento não deveria instanciar nada, até porque o
  `cargo loco routes` a chama só para listar caminhos.
- **O `JWT_SECRET` default de desenvolvimento nunca funcionou.** O valor no
  `development.yaml` (`ZGV2LW9ubHktand0LXNlY3JldA`) tem 26 caracteres e faltava
  o `==` de padding; o `EncodingKey::from_base64_secret` do Loco recusa com
  "Invalid padding", e a falha só aparecia na primeira tentativa de assinar um
  token — um 500 no `register`/`login` de qualquer instalação que não exportasse
  a variável.

---

## 1. Como ler este documento

O código Rust não é o problema — o problema é o **contrato** que ele reproduz.
No levantamento havia **233 menções ao Adonis em 65 arquivos** de
`backend/src/`, e a maioria não é comentário solto: é uma decisão de formato, de
algoritmo ou de schema que existe porque o outro backend fazia assim.

Tamanho do território (levantamento original; depois da seta, o estado após as
fases 0 a 9):

| Área | Volume |
|---|---|
| `backend/src/` | 31.668 linhas |
| `backend/tests/` | 5.751 linhas |
| `backend/migration/src/` | 1.097 linhas → **629**, numa migration só |
| `contract-tests/` | 6.190 linhas + 63 golden files (a apagar) |
| Menções ao Adonis em `backend/` | 233 → **136** (mais 19 a VineJS/Lucid e 23 a "golden") |

**O que este roadmap NÃO toca.** O domínio do produto — dump/restore, adapters
de storage (S3/GCS/Azure/SFTP/local), Docker manager, retenção GFS, agendador,
métricas — não é adaptação do Adonis. É valor próprio, já escrito em Rust
idiomático sobre o Loco. Sai intacto. O alvo é a **casca**: autenticação,
hashing, criptografia, formato de resposta, middlewares, transporte de eventos
e schema.

---

## 2. Inventário das adaptações

Cada linha foi verificada no código, não herdada de documento anterior.

A coluna **Estado** reflete a execução das fases 0 a 9.

| # | Adaptação | Vira | Estado |
|---|---|---|---|
| A | **Token opaco `oat_<id>.<secret>`** com SHA-256 na tabela `auth_access_tokens`, réplica do `DbAccessTokensProvider` | `loco_rs::auth::JWT` | ✅ Fase 2 |
| B | **scrypt** com parâmetros do `@adonisjs/hash` (`$scrypt$n=16384,r=8,p=1$…`) | `loco_rs::hash` (argon2) | ✅ Fase 3 |
| C | **AES-256-GCM fora do padrão**: IV de 16 bytes (não 12), chave crua sem KDF, formato `b64(iv):b64(tag):b64(ct)` | `Aes256Gcm` padrão, nonce de 12 bytes, chave derivada | ✅ Fase 4 |
| D | **Envelope `{success, data}`** em três variantes | payload direto via `format::json` | ✅ Fase 5 |
| E | **Duas famílias de erro** — `{errors:[…]}` do VineJS e `{success:false,message}` escrito à mão | `loco_rs::Error` + um shape único | ✅ Fase 5 |
| F | **Paginação do `SimplePaginator` do Lucid** — 9 chaves em `meta`, com `nextPageUrl` que ignora os filtros | paginação do Loco | ✅ Fase 5 — `query::fetch_page` + `Pager` |
| G | **Timestamp local ingênuo sem fuso** (`2026-08-06T16:49:25.000`) porque o Lucid gravava assim | RFC 3339 em UTC | ✅ antecipado na Fase 1 |
| H | **Mensagens e nomes de regra do VineJS** replicados à mão, sem `derive` | `validator` com `derive` | ✅ Fase 5 — `derive` onde a regra é do campo; `impl Validate` onde depende de outro campo ou do banco |
| I | **Rate limit de janela fixa em memória** reproduzindo `rate-limiter-flexible` | `tower-governor` | ✅ Fase 6 — e a chave do limitador `auth` mudou de (IP, e-mail) para IP |
| J | **`force_json`** — porte do `force_json_response` | deletar | ✅ Fase 6 — o problema real era o fallback da SPA capturar `/api` |
| K | **SSE em `/__transmit/*`** com o protocolo do `@adonisjs/transmit` | SSE próprio em `/api/events` | ✅ Fase 7 |
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
- ⚠️ **`src/dtos/` tem só `common.rs`.** Os dois bindings antigos (`ApiError.ts`,
  `Page.ts`) descreviam formas que a API **nunca** teve — `{code, message,
  details}` e `{items, total, page, per_page}` — e ninguém os importava: eram
  documentação errada que não quebrava nada por não ser usada. Foram
  substituídos na Fase 8 pelas cinco formas reais, agora importadas pelo
  `types/api.ts` e verificadas por teste contra o que o framework serializa. As
  views por recurso continuam à mão.
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
              Fase 5   Contrato HTTP          ✅
                      │
         ┌────────────┼────────────┐
         ▼            ▼            ▼
     Fase 6       Fase 7       Fase 8      ✅ ✅ ⚠️
   Middlewares      SSE      DTOs/ts-rs
         └────────────┼────────────┘
                      ▼
              Fase 9   Config e limpeza final   ⚠️ falta a varredura
                      ▼
              Fase 10  Suíte de testes          ◄── próximo
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

## Fase 5 — Contrato HTTP ✅

A fase mais larga: tocou todos os controllers e views. É onde o "cheiro de
Adonis" sumiu da API.

**Remover**
- [x] `src/views/envelope.rs` — `{success, data}` nas três variantes
- [x] `src/views/errors.rs` — as duas famílias de erro
- [x] `src/views/pagination.rs` — as 9 chaves do `SimplePaginator`, incluindo o
      `nextPageUrl` que ignorava filtros
- [x] `src/views/timestamp.rs` — timestamp sem fuso **(feito na Fase 1)**
- [x] `src/models/validation.rs` — mensagens e nomes de regra do VineJS. O
      arquivo **não** sumiu: sobrou nele o que o `derive` não alcança, e só
      isso — regra que depende de outro campo (quais chaves de `config` são
      obrigatórias depende do `provider`) e regra que precisa do banco
      (unicidade de e-mail). Os códigos agora são os do `validator`
      (`required`, `length`, `range`, `enum`), e as mensagens estão em português

**Adotar**
- [x] `format::json(payload)` direto, sem envelope; o status HTTP carrega o
      sucesso/falha
- [x] `loco_rs::Error` como erro de aplicação, com **um** shape de erro:
      `{error, description}`, e `{errors: {campo: [...]}}` quando é validação
- [x] `validator` com `derive` nos `Params` de `users`; os demais mantêm
      `impl Validate` **porque a regra não é do campo** — está documentado em
      `models/validation.rs`
- [x] `chrono` serializando RFC 3339 em UTC; o frontend formata para local
      **(feito na Fase 1)** — `formatDateTimePtBR` já usa
      `new Date(x).toLocaleString`, que lê o deslocamento e renderiza no fuso do
      navegador, então esta parte não exigiu mudança no frontend
- [x] Paginação do Loco: `query::fetch_page` nos models, `Pager` na resposta

> **Um vazamento encontrado e fechado no caminho.** Os extractors
> `JsonValidateWithMessage`/`QueryValidateWithMessage` do Loco serializam o mapa
> `params` do `validator` como está — e o `derive` grava ali o **valor
> enviado**. Uma senha do tamanho errado voltava dentro do corpo do 400, e daí
> para o log de acesso, o proxy e o rastreador de erros. Por isso os handlers
> usam `Json<T>` + `validate()` explícito, passando por
> `controllers::validation_failed`, que remove `value` e preserva o resto
> (`min`, `max`, `choices` — o que a tela usa). Há teste que trava isso.

**Outras decisões desta fase, com o porquê:**

- **Credencial inválida virou 401**, não mais 400. O 400 era um traço do
  `E_INVALID_CREDENTIALS`; 401 é o que o status significa.
- **Falha de validação virou 400**, não mais 422 — é o que o
  `Error::Validation` do Loco emite, e unificar evitou um terceiro shape.
- **Backup parcial responde 200**, não mais 422. A requisição fez o que foi
  pedido: rodou os *n* backups e relata cada um em `backups`, com
  `successful`/`failed` no topo. Era o único ponto da API em que dados úteis
  vinham dentro de um corpo de erro.
- **`PATCH /api/users/:id/status` não devolve mais mensagem**: `isActive` já diz
  o que aconteceu, e o texto da notificação é da interface, que fala o idioma do
  usuário.
- **O erro de provider desconhecido saiu do campo vazio.** Era
  `{"field":"","rule":"unionGroup"}`; agora é `errors.provider` com a lista de
  providers aceitos em `params.choices` — que é o que a tela usa para remontar o
  select.

**Quebra:** **toda** resposta da API. No frontend, 25 arquivos liam
`success`/`data`/`lastPage`/`perPage`; todos foram convertidos.
[api.ts](frontend/src/services/api.ts) deixou de desembrulhar envelope — cada
método devolve o recurso, e não `ApiResponse<T>`.

**Pronto:** nenhuma resposta contém `success`, os timestamps têm fuso, e o
frontend consome a API nova **sem camada de tradução**. `npm run type-check` e
`npm run build` verdes. ✅

**Risco:** alto. **Custo:** alto — foi a fase que dominou o cronograma.

---

## Fase 6 — Middlewares ✅

**Remover**
- [x] `src/controllers/middlewares/force_json.rs`. Ele remendava **na saída** um
      problema que estava **na rota**: o fallback do router serve a SPA, então
      `GET /api/typo` respondia `200 text/html` com a página inteira do Vue. A
      correção foi um catch-all `/api/{*path}` em `controllers::public`, que
      devolve 404 em JSON — o `matchit` prefere a rota mais específica, então
      ele só é alcançado quando nenhuma outra casou
- [x] `src/controllers/middlewares/rate_limit.rs` — a janela fixa aceitava
      **duas vezes** o limite em torno da virada do minuto (5 às 12:00:59 e
      mais 5 às 12:01:00)

**Adotar**
- [x] `tower-governor` (GCRA: um balde de `requests` fichas que repõe uma a cada
      `duration / requests`, sem virada de janela)
- [x] Os middlewares que o Loco já traz, ligados no `server.middlewares` do
      YAML: `catch_panic`, `request_id`, `limit_payload` (512 MB),
      `secure_headers` (preset `github`). O `fallback` do Loco foi **desligado**
      — a SPA e o catch-all de `/api` já cobrem os dois casos. Em `test.yaml`
      ficam todos desligados, com a razão escrita no arquivo

**Quebra, e ela é maior do que "headers e corpo do 429":**

1. **A chave do limitador `auth` deixou de incluir o e-mail.** O extrator de
   chave do `tower-governor` só vê as partes da requisição, e o e-mail está no
   corpo — ler o corpo antes de decidir bloquear era o que tornava o limitador
   antigo um vetor de ataque por si só (daí o teto de 1 MB que ele carregava).
   O saldo não é só perda: `5` por `(IP, e-mail)` dava tentativas **ilimitadas**
   a quem varria uma lista de endereços, porque trocar de e-mail zerava o
   contador. Agora conta por IP, e por isso o limite subiu de **5 para 20** por
   minuto — cinco por minuto para um escritório inteiro atrás de um NAT tranca a
   porta na primeira pessoa que erra a senha duas vezes.
2. **Só os limitadores de rota escrevem `X-RateLimit-*`.** As camadas do Axum
   montam a resposta de dentro para fora, então o limitador global — o mais
   externo — escreveria por último e apagaria o teto anunciado pela rota:
   `POST /api/auth/login` diria 600 em vez de 20. O global passou a rodar sem
   cabeçalho. `Retry-After` é escrito à mão e sai nos dois casos.
3. **O corpo do 429** virou `{"error":"too_many_requests","description":…}`, o
   mesmo shape de todo erro.

**Pronto:** `src/controllers/middlewares/` tem `layers.rs` (uma camada),
`limiters.rs` e `origin.rs` — o extractor de IP/agente para a auditoria, que o
Loco não cobre. ✅

**Risco:** baixo. **Custo:** baixo.

---

## Fase 7 — Eventos SSE próprios ✅

**Remover**
- [x] `src/controllers/transmit.rs` — handshake, `uid`, `$$transmit/ping`,
      `subscribe`/`unsubscribe`
- [x] `@adonisjs/transmit-client` do `frontend/package.json` (e do
      `package-lock.json`)
- [x] `frontend/src/plugins/transmit.ts` e o proxy `/__transmit` do
      `vite.config.mts`

**Adotar**
- [x] `axum::response::sse` em `GET /api/events?channels=a,b,c`. O nome do canal
      vai no campo `event:` do SSE, que é exatamente o que o `EventSource` usa
      para despachar por `addEventListener(canal, …)`. Sem handshake, sem `uid`,
      sem rotas de `subscribe`/`unsubscribe`: quem quer trocar de canal reabre a
      conexão, e o servidor deixa de ter estado por cliente para envelhecer
- [x] `EventSource` nativo no frontend, em `services/events.ts` — **uma**
      conexão para o aplicativo inteiro. O navegador limita ~6 conexões por
      origem em HTTP/1.1; uma por componente encostaria no teto e as requisições
      normais ficariam na fila atrás de fluxos que nunca terminam

**Duas coisas que precisaram de cuidado:**

- **A contagem de ouvintes é o que desliga a coleta de métricas.** O poller de
  recursos só coleta CPU, memória e containers quando há alguém ouvindo o canal.
  Com o `subscribe`/`unsubscribe` fora, a contagem passou a ser mantida por um
  guarda com `Drop` (`sse::Listener`): uma conexão que cai sem avisar decrementa
  do mesmo jeito. Verificado com o servidor de pé — abrir o fluxo faz as
  métricas começarem a chegar; fechar, pararem.
- **O teste não usa o `TestServer`.** Ele lê o corpo inteiro antes de devolver a
  resposta, e um fluxo SSE só termina quando o cliente desconecta — a chamada
  nunca voltaria. `tests/requests/events.rs` sobe o router num socket efêmero e
  lê os primeiros bytes, que é o que um `EventSource` faz.

> **Esta rota não exige sessão, e continua não exigindo.** O `EventSource` não
> permite cabeçalho `Authorization`, e as alternativas (token na query, cookie)
> têm cada uma o seu custo. O fluxo do `transmit` era aberto e este também é —
> **não houve regressão**, mas fica registrado como pendência de segurança. É
> decisão de produto, não efeito colateral de um porte.

**Pronto:** `grep -ri adonis frontend/src frontend/package.json` volta vazio. ✅

**Risco:** médio. **Custo:** médio.

---

## Fase 8 — Contrato tipado em `dtos/` ⚠️ parcial

Não é remoção de legado: é a regra do próprio [AGENTS.md §5](backend/AGENTS.md)
que nunca foi cumprida, porque os `views/` estavam ocupados replicando shapes.
Com a Fase 5 feita, o caminho abre.

**Pela decisão 3, esta fase deixou de ser opcional.** Com a OpenAPI removida na
Fase 1, os bindings `ts-rs` passam a ser a **única descrição formal** do que a
API aceita e devolve. A diferença a favor: uma spec Swagger desatualizada mente
em silêncio; um binding desatualizado **quebra o build do frontend**.

**Adotar**
- [x] As formas comuns a **toda** resposta em `src/dtos/common.rs`, geradas para
      `frontend/src/bindings/`: `Paginated<T>`, `PageInfo`, `ApiErrorBody`,
      `FieldError`, `MessageResponse`
- [x] Frontend importa de `@/bindings/` em vez de tipar à mão — `types/api.ts`
      reexporta os cinco
- [x] O CI regera e reprova binding fora de data
      (`.github/workflows/ci.yml`, job `bindings`)
- [ ] As views por recurso (`Connection`, `Backup`, `Storage`, `AuditLog`, …)
      ainda são tipadas à mão no frontend

> **Por que os DTOs redeclaram tipos que o framework já tem.** `Pager` e
> `ErrorDetail` são do `loco_rs` e não derivam `TS`; não dá para acrescentar um
> derive a um tipo de outro crate. A alternativa era o frontend redigitar os
> campos, que é exatamente o que esta fase existe para acabar. O risco da
> duplicação — as duas descrições divergirem — é fechado por três testes em
> `dtos/common.rs` que comparam a struct com o que o framework **de fato**
> serializa. Uma divergência quebra o build.

**Pronto quando:** todo endpoint consumido pelo frontend tem binding gerado. O
que falta é mecânico e repetitivo: mover cada `views::*` para `dtos/` com o
derive. A parte que dava a garantia — o CI que reprova binding fora de data —
já está de pé.

**Risco:** baixo. **Custo:** médio.

---

## Fase 9 — Configuração e limpeza final

- [x] **`auth.jwt.secret` fora do YAML versionado** — saiu na Fase 2, junto com
      `JWT_EXPIRATION`, o `.env.example` e os dois `docker-compose`
- [x] Revisar `config/*.yaml` contra o scaffold do Loco. Saiu
      `auth_access_token_expires_in`; entrou o bloco `server.middlewares` de
      verdade (antes só tinha `fallback: true`), com o motivo de cada
      middleware escrito no arquivo; o `secret` do JWT ganhou o padding que
      faltava
- [ ] Remover as **menções ao Adonis** em `backend/src/`. Regra: o comentário
      que explica *por que o código é assim* perde o objeto quando o "assim"
      deixa de existir. O que sobreviver deve justificar a decisão **em si**,
      sem citar o framework antigo.
      **Estado: 136 menções a "Adonis", 19 a VineJS/Lucid e 23 a "golden"**,
      em `backend/src/` e `backend/tests/` (eram 233 no levantamento).
      **Deliberadamente não varridas em massa.** A maioria está em `views/`, e
      lá as menções ainda descrevem uma decisão real: os campos que cada view
      emite não mudaram na Fase 5 — só o envelope em volta saiu. Trocar
      "o Adonis omite a chave" por "omite a chave" produziria um comentário que
      não explica nada, que é pior do que um que cita um framework morto.
      O trabalho certo é reescrever cada um justificando a decisão em si, um a
      um, e isso é uma tarefa própria — não um `sed`
- [x] `nodeVersion` → `runtimeVersion` em `/api/system/status` (backend, o
      binding TS e o rótulo "Node.js" do `SystemInfoCard.vue`)
- [x] `GET /api/health`: passou a usar `App::app_version()` no lugar do
      `"1.0.0"` fixo — a versão é justamente o que se pergunta a um health
      check durante um incidente
- [x] Reescrever `backend/AGENTS.md` §10.9 (a exigência `contract:roco`) e a
      seção de suíte de contrato do `backend/README.md`
- [x] Atualizar as árvores de `README.md` e `CHECKLIST.md`
- [ ] `DOCKER_MANAGER_CHECKLIST.md` — 346 linhas, **zero menções ao Adonis**;
      a revisão pendente é de conteúdo (o que ainda vale), não de porte
- [x] `.github/workflows/ci.yml` no lugar de `contract-tests.yml`: formatação,
      clippy, testes, bindings em dia, lint e build do frontend

**Pronto quando:** `grep -ri adonis` no repositório volta **vazio**. Hoje volta
vazio em `frontend/`; falta `backend/`.

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
| 5 | **Toda** leitura de resposta: `success`, `data`, `meta.lastPage`, `perPage`, parsing de timestamp | ✅ 25 arquivos convertidos; `api.ts` devolve o recurso, não `ApiResponse<T>`. O parsing de timestamp foi absorvido sem mudança (RFC 3339 e `toLocaleString`) |
| 6 | Tratamento de 429 | ✅ `extractErrorMessage` lê `description`; o 429 tem o mesmo shape dos demais erros |
| 7 | Notificações em tempo real; remoção do `@adonisjs/transmit-client` | ✅ `services/events.ts` com `EventSource`, uma conexão para o app inteiro |
| 8 | Tipos passam a vir de `src/bindings/` | ⚠️ parcial — as formas comuns sim; as views por recurso ainda são tipadas à mão |

Ponto de concentração: [frontend/src/services/api.ts](frontend/src/services/api.ts).
Foi tratado como a fronteira e adaptado primeiro em cada fase.

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
   (o único pendente do marco da Fase 1; o `ci.yml` que os substitui já está no
   lugar, então o `contract-tests.yml` hoje só reprova o PR por testar um
   contrato que não existe mais)
2. Definir `JWT_SECRET` no ambiente de produção antes do deploy
3. Subir um SMTP de desenvolvimento e confirmar a chegada do e-mail de `forgot`
   — o único caminho das fases 0–9 que nunca foi exercitado de ponta a ponta
4. **Fase 10 — suíte de testes.** É o próximo item da fila, e o mais urgente:
   `tests/requests/*` foi convertido para o contrato novo (155 testes verdes),
   mas convertido é diferente de *revisado*. As asserções continuam sendo as que
   descreviam a API antiga, com os campos trocados
5. Terminar a Fase 8: mover cada `views::*` para `dtos/` com o derive `TS`
6. Fazer a varredura das menções ao Adonis, uma a uma (Fase 9)
7. Decidir se o fluxo SSE deve exigir sessão (Fase 7)
