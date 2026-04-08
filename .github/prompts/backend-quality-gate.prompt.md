---
description: "Executa o quality gate completo do backend: typecheck, lint e testes funcionais. Use ao finalizar qualquer feature ou correção no backend."
name: "Backend Quality Gate"
argument-hint: "Descreva brevemente a feature finalizada (opcional)"
agent: "agent"
---

Execute o quality gate completo do backend em sequência. Corrija todos os erros encontrados antes de considerar a tarefa concluída.

## Passos

### 1. TypeScript — Zero erros

```bash
cd backend
pnpm typecheck
```

- Se houver erros de tipo, corrija-os antes de continuar.
- Não prossiga para o próximo passo enquanto houver erros.

### 2. ESLint — Zero erros

```bash
cd backend
pnpm lint
```

- Corrija todos os erros de lint.
- Avisos (`warn`) não bloqueiam, mas devem ser avaliados.

### 3. Testes Funcionais — Todos passando

```bash
cd backend
pnpm test
```

- Se algum teste falhar, investigue e corrija a causa raiz.
- Não adicione `skip` ou `todo` para mascarar falhas.
- Se a feature alterou comportamento já coberto por testes existentes, atualize os testes para refletir o novo contrato.

## Critério de Conclusão

A tarefa só está **concluída** quando os três comandos acima terminam com **exit code 0** e zero erros/falhas reportados.

## Observações

- Path aliases do backend: `#services/*`, `#models/*`, `#controllers/*` — nunca use caminhos relativos entre módulos.
- Testes ficam em `backend/tests/functional/`. Novos testes devem seguir o padrão Japa com `apiClient`.
- Rode dentro do container se preferir: `node ace test` (equivalente ao `pnpm test`).
