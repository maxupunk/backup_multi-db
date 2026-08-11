# Guia do agente — aplicação Loco (loco.rs)

Esta é uma aplicação **Loco** (loco.rs), framework Rust *batteries-included*.
Roteamento, banco (Sea-ORM), background jobs, scheduler, mailers, tasks, storage,
cache e testes **já vêm integrados**.

> **Diretriz principal:** não saia do padrão. Antes de escrever qualquer código,
> verifique se o Loco já resolve o problema. Só introduza crate externo, camada
> nova ou infraestrutura manual quando o framework comprovadamente não cobrir o
> caso — e registre o motivo no PR.

---

## 1. Fontes de verdade (consulte antes de inventar)

Quando houver dúvida sobre API, assinatura ou convenção, **consulte a
documentação antes de improvisar**. Nunca invente um método do `loco_rs`.

- Guia oficial para agentes: https://loco.rs/AGENTS.md
- Referência completa em arquivo único: https://loco.rs/llms-full.txt
- Documentação: https://loco.rs/docs
- Sea-ORM: https://www.sea-ql.org/SeaORM/docs/index/

Ordem de precedência quando as fontes divergirem:
**código existente neste repositório → docs do Loco → docs do Sea-ORM/Axum → sua intuição.**

---

## 2. Mapa do projeto

```
src/app.rs             # impl Hooks for App — registra rotas/workers/tasks (hub de wiring)
src/controllers/       # handlers HTTP agrupados em Routes
src/models/_entities/  # entidades Sea-ORM GERADAS — nunca editar à mão
src/models/*.rs        # lógica de domínio dos models
src/views/             # serialização de resposta (structs de saída)
src/dtos/              # contrato tipado da API (derive ts-rs -> bindings do frontend)
src/workers/           # jobs em background
src/tasks/             # tasks de CLI/admin
src/mailers/           # e-mail (código + templates .t)
src/initializers/      # inicializadores do boot
src/data/              # dados estáticos embarcados (include_dir)
src/fixtures/          # seeds YAML
migration/             # migrations Sea-ORM
config/*.yaml          # configuração por ambiente (LOCO_ENV)
tests/                 # testes de request/model/task/worker
```

Regra de ouro: **cada coisa no seu lugar.** Se um arquivo novo não se encaixa em
nenhuma dessas pastas, provavelmente a modelagem está errada — repense antes de
criar uma pasta nova.

---

## 3. Arquitetura em camadas

Fluxo de uma requisição — sempre nesta direção, sem atalhos:

```
Routes (controllers/x.rs::routes)
   -> Controller  : extrai/valida entrada, autentica, traduz erros em HTTP
      -> Model    : regra de negócio + acesso a dados (Sea-ORM)
         -> View/DTO : monta a resposta serializável
```

Responsabilidades, sem sobreposição:

| Camada | Faz | Não faz |
|---|---|---|
| Controller | extrair params, autorizar, orquestrar chamadas, devolver `Response` | consultar o banco direto, montar SQL, conter regra de negócio |
| Model | validação de domínio, transações, queries, invariantes | conhecer HTTP, status code, `Json`, headers |
| View/DTO | mapear `Model` -> payload de saída | consultar banco, decidir regra |
| Worker/Task | trabalho assíncrono/batch reusando os models | duplicar regra já existente no model |

- Query no controller é **anti-padrão**: mova para um método associado do model
  (ex.: `users::Model::find_by_email(&ctx.db, email)`).
- Nada de `models/_entities/` editado à mão. Estenda em `src/models/<nome>.rs`
  via `impl Model` / `impl ActiveModel` / `ActiveModelBehavior`.

---

## 4. SOLID aplicado a este projeto

Os princípios valem, mas **traduzidos para o idioma do Loco** — não importe
padrões de Java/C# (fábricas, containers de DI, repositórios genéricos) que o
framework já resolve de outra forma.

**S — Responsabilidade única**
Um handler faz uma coisa. Um método de model resolve uma operação de domínio.
Se uma função passa de ~50 linhas ou tem mais de um motivo para mudar, quebre.
Arquivo de controller por recurso; arquivo de model por entidade.

**O — Aberto/fechado**
Estenda pelos pontos de extensão do framework — `Hooks`, `Initializer`,
`ActiveModelBehavior`, middlewares, novos `Routes` — em vez de alterar código
central existente. Adicionar recurso = adicionar arquivo + registrar em
`src/app.rs`, não reescrever o que já funciona.

**L — Substituição de Liskov**
Ao implementar um trait do Loco (`Hooks`, `BackgroundWorker`, `Task`, `Mailer`),
respeite o contrato: mesmos tipos de erro, mesma semântica, sem panics onde se
espera `Result`. Uma implementação não pode surpreender quem chama pelo trait.

**I — Segregação de interfaces**
Traits pequenos e focados. Structs de parâmetro específicas por operação
(`LoginParams`, `RegisterParams`, `ResetParams`) em vez de um "mega-params" com
campos `Option` para todo mundo. O mesmo vale para as views.

**D — Inversão de dependência**
O `AppContext` **é** o mecanismo de injeção deste framework. Dependa dele
(`ctx.db`, `ctx.mailer`, `ctx.storage`, `ctx.cache`, `ctx.queue_provider`,
`ctx.config`), nunca de singletons globais, pools próprios ou `static mut`.
Funções de domínio recebem `&impl ConnectionTrait` / `&DatabaseConnection` como
parâmetro — assim rodam em transação e em teste sem alteração.

Antes de abstrair, pergunte: **existe hoje um segundo caso de uso real?** Se não,
não crie o trait. Abstração especulativa é dívida, não SOLID.

---

## 5. Regras de padrão (não negociáveis)

- **Gere, depois edite.** Toda feature nova começa por
  `cargo loco generate model|scaffold|controller|worker|task|mailer|migration ...`.
  Os generators também fazem o wiring em `src/app.rs`.
- **Tudo passa pelo `AppContext` (`ctx`).** Não crie pool de banco, servidor HTTP,
  fila de jobs ou cliente de e-mail próprio.
- Todo controller/model/worker/task começa com `use loco_rs::prelude::*;`.
- Código de aplicação retorna `loco_rs::Result<T>` e propaga com `?`.
- Respostas HTTP saem por `format::json(...)`, `format::empty_json()`,
  `unauthorized(...)`, `bad_request(...)` — não monte `Response` na mão.
- Handlers levam `#[debug_handler]`.
- Chaves primárias e estrangeiras são `i64`.
- Configuração é YAML em `config/`; **segredo nunca em arquivo** — use o helper
  Tera `get_env` dentro do YAML.
- Migration: sempre gerada, com o nome no formato `mYYYYMMDD_HHMMSS_<assunto>.rs`,
  e registrada em `migration/src/lib.rs`. Migration é *append-only*: nunca edite
  uma já aplicada — crie outra.
- Depois de mexer em schema: rode a migration e **regenere as entidades**; não
  ajuste `_entities/` manualmente.
- Novo contrato consumido pelo frontend vai em `src/dtos/` com
  `#[derive(TS)] #[ts(export, export_to = "../frontend/src/bindings/")]`.
  Views (`src/views/`) são respostas específicas de recurso, sem lógica.
- Rota nova = registrada em `controllers::<recurso>::routes()` com `.prefix("/api/...")`
  e adicionada em `App::routes`.

---

## 6. Convenções de código Rust

- `cargo fmt` obrigatório (`max_width = 100`, definido em `.rustfmt.toml`).
- `cargo clippy --all-targets -- -D warnings` limpo antes de concluir.
- **Sem `unwrap()`/`expect()`/`panic!` em caminho de request.** Use `?`,
  `let ... else`, ou converta em erro do Loco. `expect` só é aceitável em
  inicialização estática comprovadamente infalível (ex.: regex constante).
- Sem `unsafe`. Sem `.clone()` defensivo — empreste (`&`) quando der.
- Nomes: `snake_case` para funções/campos, `CamelCase` para tipos,
  `SCREAMING_SNAKE_CASE` para consts. Nada de abreviação obscura.
- Log estruturado com `tracing`, campos nomeados:
  `tracing::info!(pid = user.pid.to_string(), "user verified")`. Nunca logue
  senha, token, hash ou payload inteiro.
- Doc comment (`///`) em todo item público não trivial — em especial handlers,
  descrevendo o fluxo e as respostas.
- Comentários explicam **por quê**, não o quê.
- Idioma: **código, identificadores e comentários em inglês**; documentação de
  projeto (`*.md`) em português.

---

## 7. Testes

Nenhuma feature está pronta sem teste. Espelhe a estrutura de `src/` em `tests/`.

- Request: `request::<App, _, _>(|request, ctx| async move { ... }).await;`
- Model/Task: use os helpers de `loco_rs::testing`, com `boot_test` e seeds.
- Snapshots com `insta` (`cargo insta review` para aprovar mudanças conscientes —
  nunca aceite snapshot novo sem ler o diff).
- `#[serial]` (`serial_test`) em teste que toca o banco compartilhado.
- Casos parametrizados com `rstest`.
- Cubra sempre: caminho feliz, entrada inválida e caminho não autorizado.

---

## 8. Segurança

- Endpoint autenticado recebe o extractor `auth::JWT`; a autorização é decidida
  no controller, explicitamente.
- Não vaze existência de usuário: fluxos de e-mail (`forgot`, `magic-link`,
  `resend-verification`) respondem sucesso mesmo quando o e-mail não existe —
  mantenha esse comportamento.
- Validação de entrada com `validator` no model (`Validatable` / `Validator`),
  não espalhada pelo controller.
- Segredos só via ambiente (`get_env` no YAML). Nada de credencial versionada.
- Toda query é parametrizada via Sea-ORM. SQL cru só com justificativa explícita.

---

## 9. Anti-padrões — nunca faça

- Adicionar crate para algo que o Loco já entrega (web server, fila, cache, cron).
- Acessar `ctx.db` direto do controller para montar query ad-hoc.
- Editar `src/models/_entities/**` ou uma migration já aplicada.
- Criar camada de "service"/"repository" genérica em cima dos models.
- `unwrap()` em handler, `.await` dentro de loop quando existe query em lote.
- Espalhar `String` de erro solta em vez de usar os erros do `loco_rs`.
- Criar pasta nova na raiz de `src/` fora do mapa da seção 2.
- Deixar `dbg!`, `println!` ou código comentado no commit.

---

## 10. Definição de pronto (checklist do PR)

1. Feature nasceu de generator quando havia um aplicável.
2. Camadas respeitadas (seção 3) e responsabilidade única (seção 4).
3. Migration criada, aplicada e entidades regeneradas — se houve schema.
4. Rota/worker/task registrada em `src/app.rs`.
5. DTO exportado para o frontend, quando o contrato é consumido por ele.
6. Testes novos passando: `cargo test`.
7. `cargo fmt` + `cargo clippy --all-targets -- -D warnings` limpos.
8. Nenhum segredo, log sensível ou `unwrap` no diff.
9. Suíte de contrato passando: `cd contract-tests && pnpm contract:roco`.

---

## 11. Comandos úteis

```sh
cargo loco start                   # sobe a aplicação
cargo loco generate <tipo> <args>  # scaffold/model/controller/worker/task/mailer/migration
cargo loco db migrate              # aplica migrations
cargo loco db entities             # regenera src/models/_entities
cargo loco routes                  # lista as rotas
cargo loco task <nome>             # executa uma task
cargo loco doctor                  # checa o ambiente
cargo loco-tool <args>             # binário auxiliar (src/bin/tool.rs)
cargo playground                   # examples/playground.rs
cargo test                         # suíte completa
cargo fmt && cargo clippy --all-targets -- -D warnings
```

---

## 12. Quando o padrão não cobrir o caso

Se a solução exigir sair do que está escrito aqui: **pare e explique antes de
implementar** — qual regra está sendo quebrada, por quê o caminho padrão não
serve, e qual o custo da alternativa. Desvio combinado vira uma nova regra nesta
página; desvio silencioso vira dívida.
