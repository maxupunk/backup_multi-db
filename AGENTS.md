# Regras e Convenções para Agentes de Desenvolvimento (AGENTS.md)

Este repositório possui regras estritas de arquitetura limpa, manutenibilidade e qualidade de código. Todos os agentes devem ler e seguir rigorosamente as diretrizes abaixo:

## 1. Aproveitamento de Código e Unificação de Componentes (DRY)
- **Centralização de Cores e Status:** Não crie resolvers locais de cor de métricas ou mapeamento de status em componentes individuais.
  - Para métricas de CPU/Memória e estados Docker: utilize `@/utils/dockerMetrics.ts` (`resolveUsageColor`, `resolveChartColor`, `resolveContainerStateMeta`, `resolveContainerStatusColor`).
  - Para entidades de negócio (Backup, Conexões, Storage): utilize os módulos em `@/ui/` (`backup.ts`, `database.ts`, `storage.ts`).
- **Componentes Compartilhados de Diálogo e Ação:** Diálogos repetidos (como `DockerPruneDialog.vue`, `DockerActionConfirmDialog.vue`, `ContainerRemoveDialog.vue`) devem ser centralizados em `@/components/`, encapsulando seu estado interno e emitindo `@success`.

## 2. Separação de Responsabilidades (Composables & SOLID)
- Lógicas complexas de dados, polling, filtros e transformações devem estar isoladas em `@/composables/` (ex: `useContainerProcesses.ts`, `useDockerContainerResources.ts`).
- Componentes `.vue` devem ser concisos e voltados para apresentação e layout.
- Timers (`setInterval`) e assinaturas de eventos SSE **devem** ser finalizados no hook `onUnmounted` para prevenir memory leaks.

## 3. Experiência do Usuário (Anti-Flicker & Zero Layout Shift)
- **Atualização Silenciosa:** O polling periódico deve operar com `silent = true` sem ativar loaders invasivos ou desmontar a árvore do DOM.
- **Chaves Estáveis:** Use identificadores únicos nas iterações `v-for` (ex: `:key="'proc-' + item.pid"` ou `:key="container.id"`). **Nunca** use o índice do array (`:key="index"`).
- **Alinhamento Numérico:** Use `font-variant-numeric: tabular-nums` para valores numéricos oscilantes a fim de evitar trepidação no layout.

## 4. Tipagem e Validação
- **TypeScript Estrito:** Programação defensiva com checagem explícita de nulidade e sem uso de `any`.
- **Validação:** Sempre execute e assegure aprovação em:
  - Frontend: `npm run type-check` (`vue-tsc --build --force`)
  - Backend: `cargo check`
