# contract-tests

Suíte de contrato **black-box** do backup_multi-db. Roda por HTTP, sem importar
nada de nenhuma das duas implementações, e por isso executa idêntica contra o
`backend/` (AdonisJS) e contra o `back-roco/` (Rust/Loco).

É a Fase 1 do [ROADMAP_BACK_ROCO.md](../ROADMAP_BACK_ROCO.md). A Fase 2 usa esta
infraestrutura para cobrir os 87 pares método+rota de `/api`.

## Como usar

```bash
cd contract-tests
pnpm install

pnpm contract:selftest    # testa os matchers do próprio harness (sem servidor)
pnpm contract:record      # sobe o Adonis e GRAVA os golden files
pnpm contract:adonis      # roda contra o Adonis e COMPARA com os golden
pnpm contract:roco        # o mesmo, contra o back-roco
pnpm contract:diff        # contra o back-roco + reports/contract-diff.md
pnpm contract:coverage    # reprova se alguma rota do baseline ficar sem teste
```

Filtro do vitest passa direto: `pnpm contract:adonis -- -t "health"`.

## O que o harness faz por execução

1. Recria `.contract/<runId>/` — banco SQLite descartável, logs e estado.
2. Roda as migrations e **confere que o banco nasceu dentro desse diretório**.
3. Sobe o servidor numa porta livre e espera `GET /api/health`.
4. Semeia usuários, conexões e storages **pela própria API HTTP**.
5. Executa os testes em série contra esse servidor.
6. Derruba tudo e escreve `reports/route-coverage.md`.

### Por que o harness sobe o próprio servidor

O roadmap listava como alternativa um `POST /api/__test__/reset`. Foi
descartado por dois motivos:

- a decisão **D8** (big-bang) exige o `backend/` congelado, e abrir rota nova
  nele só para testar contraria isso;
- o rate limiter do Adonis usa store **em memória**, e o limiter de `auth` é de
  5 req/min. Um endpoint de reset limparia o banco e deixaria os contadores
  intactos. Reiniciar o processo zera as duas coisas.

### Segurança do banco

O backend lê o `.env` da raiz do repositório, onde fica o caminho do banco de
**produção**. As variáveis que o harness passa têm precedência sobre o `.env`
(`process.env` vence, verificado em `@adonisjs/env`), mas o harness não confia
nisso em silêncio: depois das migrations ele exige que o arquivo tenha nascido
dentro de `.contract/<runId>/`. Se não nasceu, aborta antes de escrever.

## Golden files

Os golden ficam em `__golden__/` e **são versionados** — é o diff deles que
mostra uma mudança de contrato entrando no repositório.

Cada arquivo guarda duas representações da resposta:

- `response.shape` — derivado do corpo **cru**. É o que a comparação usa.
- `response.body` — corpo **redigido** (id, timestamp, token → marcadores).
  Existe só para leitura humana no code review.

Manter as duas é proposital: a redação troca tipos (um `number` vira
`"<id>"`), então um golden que só guardasse o corpo redigido perderia o
contrato "`id` é número" sem ninguém perceber.

`contract:record` só funciona com `--target adonis`. Gravar golden a partir do
back-roco faria a suíte comparar a implementação com ela mesma.

## Tolerâncias da comparação

Comparação é de **formato**, não de valor. As tolerâncias são deliberadas:

| Situação | Decisão |
|---|---|
| Ordem das chaves | irrelevante (ordenadas na derivação) |
| Valor de id, data, duração | irrelevante (só o tipo importa) |
| Tamanho de array | irrelevante; o formato do item é comparado |
| Array heterogêneo | os itens são unificados, não só o item 0 |
| `null` em um dos lados | tratado como campo nulável, não como conflito |
| Array vazio onde o golden tinha itens | reportado como `unverified-array` |
| Chave a mais na resposta | **falha** (`allowExtraKeys` afrouxa) |
| `charset` do content-type | irrelevante; só o mime é comparado |

Os matchers têm teste próprio (`pnpm contract:selftest`), inclusive um que
garante que eles **reprovam** algo — um matcher que sempre aprova passaria em
todos os outros.

## Cobertura de rotas

Toda chamada feita pelo cliente HTTP é casada contra
`docs/routes-baseline.txt` e anotada. O seed **não** conta: ele não afirma nada
sobre a resposta, e contá-lo marcaria rotas como testadas sem teste algum.

O relatório sai em `reports/route-coverage.md`. Com `--enforce-coverage`, rota
sem teste reprova a execução.

## Limitações conhecidas

- **Backups não são semeados.** Criar um backup exige um banco de origem vivo
  (`docker-compose.test.yml`) e um dump real. Fica para o lote de backups da
  Fase 2, onde há teste consumindo.
- **Valores de cabeçalho não são comparados por padrão** — só a presença do
  nome e o mime. Os testes de rate limit da Fase 2 assertam
  `x-ratelimit-*` explicitamente via `assertHeaders`.
- **Os seeds de conexão apontam para o `docker-compose.test.yml`** mas não
  exigem que ele esteja de pé: criar uma conexão não abre socket. Só
  `POST /api/connections/:id/test` abre.
