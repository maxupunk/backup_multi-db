---
description: "Executa o quality gate completo do backend: fmt, clippy e testes. Use ao finalizar qualquer feature ou correção no backend."
name: "Backend Quality Gate"
argument-hint: "Descreva brevemente a feature finalizada (opcional)"
agent: "agent"
---

Execute o quality gate completo do backend em sequência. Corrija todos os erros encontrados antes de considerar a tarefa concluída.

## Passos

### 1. Formatação — Zero diferenças

```bash
cd backend
cargo fmt --check
```

- Se houver diferenças, corrija com `cargo fmt`.
- Não prossiga para o próximo passo enquanto houver erros.

### 2. Clippy — Zero warnings

```bash
cd backend
cargo clippy --all-targets -- -D warnings
```

- Corrija todos os warnings e erros do clippy.

### 3. Testes — Todos passando

```bash
cd backend
cargo test
```

- Se algum teste falhar, investigue e corrija a causa raiz.
- Não adicione `#[ignore]` ou `todo!` para mascarar falhas.
- Se a feature alterou comportamento já coberto por testes existentes, atualize os testes para refletir o novo contrato.

## Critério de Conclusão

A tarefa só está **concluída** quando os três comandos acima terminam com **exit code 0** e zero erros/falhas reportados.

## Observações

- Testes ficam em `backend/tests/`. Novos testes devem seguir o padrão do Loco.
- Rode a suíte de contrato quando alterar o contrato HTTP: `cd contract-tests && pnpm contract:roco`.
