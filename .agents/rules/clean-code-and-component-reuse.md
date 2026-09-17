# Diretrizes de Arquitetura, Reutilização de Código e UX

Este documento estabelece as regras obrigatórias de desenvolvimento para o projeto, com foco em Clean Code, manutenibilidade, DRY e experiência do usuário (UX). Todos os agentes e desenvolvedores devem seguir estas diretrizes.

---

### 1. Reutilização de Código e Unificação de Componentes (DRY & Single Source of Truth)

- **Cores, Status e Metadados Centralizados:**
  - Nunca duplique lógica de mapeamento de status, ícones ou cores de métricas (`success`, `warning`, `error`, etc.) em páginas ou componentes individuais.
  - Métricas de recursos do sistema e Docker utilizam `@/utils/dockerMetrics.ts` (`resolveUsageColor`, `resolveChartColor`, `resolveContainerStateMeta`, `resolveContainerStatusColor`).
  - Status e metadados de domínio devem residir nos módulos correspondentes em `@/ui/`:
    - `@/ui/backup.ts` para backups
    - `@/ui/database.ts` para conexões e bancos
    - `@/ui/storage.ts` para provedores de storage
- **Componentes de Ação e Diálogos Compartilhados:**
  - Se uma funcionalidade (ex: Prune do Docker, Confirmação de exclusão, Teste de conexão) aparece em mais de um local, ela **deve ser encapsulada em um componente compartilhado** em `@/components/` (ex: `DockerPruneDialog.vue`, `DockerActionConfirmDialog.vue`).
  - O componente deve gerenciar seus próprios estados de loading, mensagens de feedback via `useNotifier` e confirmações, emitindo eventos de sucesso (ex: `@success`) para o pai atualizar a lista.

---

### 2. Separação de Responsabilidades com Composables (SOLID)

- **Lógica de Dados e Polling em `@/composables/`:**
  - Regras de negócio, lifecycle de polling/timers, filtros de busca e ordenação devem ser extraídos para composables reutilizáveis (ex: `useContainerProcesses.ts`, `useDockerContainerResources.ts`, `useSystemResources.ts`).
  - O componente Vue (`.vue`) deve focar em apresentação e layout, mantendo arquivos curtos e legíveis (idealmente abaixo de 300 linhas).
- **Gerenciamento Seguro de Recursos:**
  - Qualquer polling (`setInterval`), listener ou SSE (`subscribe`) **deve** ser rigorosamente cancelado no `onUnmounted` para prevenir memory leaks.

---

### 3. Prevenção de Flicker e Zero Layout Shift (CLS = 0)

Ao atualizar métricas ou dados em tempo real:
1. **Atualizações Silenciosas em Background:**
   - Polling periódico deve rodar com flag silenciosa (`silent: boolean = true`). Nunca ative spinners globais que desmontem a tabela ou os cards durante um refresh agendado.
2. **Chaves Estáveis no `v-for`:**
   - Sempre utilize identificadores únicos e estáveis (ex: `:key="'proc-' + item.pid"`, `:key="container.id"`).
   - **Proibido** usar o índice do array (`:key="index"`), pois isso força o Vue a redesenhar elementos desnecessariamente.
3. **Imutabilidade de Estruturas Estáticas:**
   - Defina definições de colunas (`headers`, `titles`) de forma estática fora de recomputações desnecessárias para evitar recriação do `<thead>`.
4. **Números Tabulares:**
   - Para células que exibem contadores, percentuais ou métricas que mudam constantemente, utilize CSS `font-variant-numeric: tabular-nums` para evitar variações na largura dos elementos.

---

### 4. Qualidade, Tipagem e Verificação

- **TypeScript Estrito:**
  - Tipagem defensiva sem uso de `any`.
  - Tratamento explícito de nulidade (`null` e `undefined`).
  - Uso de `satisfies Record<string, T>` para mapeamentos estáticos tipados.
- **Validação Contínua:**
  - Ao alterar o frontend: execute `npm run type-check` (`vue-tsc --build --force`).
  - Ao alterar o backend em Rust: execute `cargo check`.
