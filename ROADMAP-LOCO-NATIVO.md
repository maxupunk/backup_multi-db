# Roadmap — Loco nativo

> **Estado final:** concluído em 2026-08-12.
>
> O projeto usa o contrato, autenticação, criptografia, eventos e schema nativos
> do Loco. Não há compatibilidade de dados com a implementação anterior: um
> banco novo é criado pelas migrations e os usuários e credenciais são
> cadastrados novamente.

---

## Estado da execução

| Fase | Situação |
|---|---|
| 0 — Poda do morto | ✅ concluída |
| 1 — Schema do zero | ✅ concluída |
| 1 — Marco: documentação e suíte de contrato aposentadas | ✅ concluída |
| 2 — Autenticação JWT | ✅ concluída |
| 3 — Hash argon2 e recuperação de senha | ✅ concluída |
| 4 — Criptografia padrão | ✅ concluída |
| 5 — Contrato HTTP | ✅ concluída |
| 6 — Middlewares | ✅ concluída |
| 7 — Eventos SSE próprios | ✅ concluída |
| 8 — Contrato tipado em `dtos/` | ✅ concluída |
| 9 — Configuração e limpeza final | ✅ concluída |
| 10 — Suíte de testes | ✅ concluída |

## Entregas verificadas

- [x] A suíte de contrato, seus golden files, a especificação OpenAPI e o job
  de CI correspondente foram removidos.
- [x] O Dockerfile e os documentos do projeto não referenciam mais esses
  artefatos.
- [x] A autenticação usa JWT, senhas usam argon2 e credenciais persistidas usam
  cifra AES-256-GCM com chave derivada.
- [x] A API devolve payload direto, erros uniformes, paginação `results` /
  `pagination` e timestamps RFC 3339.
- [x] Eventos usam `GET /api/events` com `EventSource` no frontend.
- [x] Os DTOs Rust geram bindings TypeScript; o frontend consome os bindings
  para usuários, conexões, backups, storages, destinos, auditoria e sistema.
- [x] O CI regenera os bindings e falha se houver divergência.
- [x] A varredura por referências à plataforma anterior no repositório retorna
  vazia.
- [x] `DOCKER_MANAGER_CHECKLIST.md` foi revisado para refletir a implementação
  atual.
- [x] A auditoria de requests cobre as 89 rotas `/api`; não há rota sem teste
  direto. Ver [ROUTE_AUDIT.md](backend/tests/requests/ROUTE_AUDIT.md).
- [x] Há snapshot revisado com `insta` para a resposta pública de healthcheck.

## Portões de qualidade

Execute a partir da raiz indicada antes de publicar:

```sh
cd backend
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test

cd ../frontend
npm run type-check
npm run build
```

## Operação de produção

Estes itens não exigem mudança no repositório, mas são obrigatórios no deploy:

- [x] O processo exige `JWT_SECRET` em produção; gere um valor com
  `openssl rand -base64 48` e configure-o no ambiente de execução.
- [x] O fluxo `forgot`/`reset` está coberto até o mailer e possui templates;
  valide a entrega no SMTP configurado durante a homologação do ambiente.
- [x] Para instalação nova, descarte o SQLite anterior e execute
  `cargo loco db migrate`.
- [x] Recadastre usuários e reinsira as credenciais de conexões e storages.

Não há itens pendentes de implementação neste roadmap.
