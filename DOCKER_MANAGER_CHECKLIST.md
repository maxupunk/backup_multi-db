# 📋 Checklist — Docker Manager Feature

> **Documento base:** `DOCKER_MANAGER.MD`
> **Criado em:** 2026-04-12
> **Objetivo:** Gerenciar containers, volumes, networks, imagens e logs do Docker via interface web integrada ao sistema.

---

## 📌 Contexto do Sistema Atual

### Infraestrutura Docker já existente (backend)

| Arquivo | Responsabilidade | Status |
|---|---|---|
| `docker_engine_http_client.ts` | Cliente HTTP via Unix socket `/var/run/docker.sock` | ✅ Existe |
| `docker_container_discovery_service.ts` | Descobre containers com banco de dados | ✅ Existe |
| `docker_container_monitoring_service.ts` | Coleta métricas de CPU/memória/rede (SSE) | ✅ Existe |
| `docker_container_resource_emitter.ts` | Emissão SSE das métricas em tempo real | ✅ Existe |
| `docker_environment_service.ts` | Detecta ambiente Docker (socket, container ID, rede) | ✅ Existe |
| `docker_discovery_types.ts` | Tipos/interfaces de descoberta | ✅ Existe |

### Infraestrutura Docker já existente (frontend)

| Arquivo | Responsabilidade | Status |
|---|---|---|
| `components/system/DockerContainerResourceCharts.vue` | Gráficos de recursos dos containers | ✅ Existe |
| `components/system/DockerContainerResourceCard.vue` | Card individual de métricas | ✅ Existe |
| `composables/useDockerContainerResources.ts` | Composable SSE de métricas | ✅ Existe |

### O que NÃO existe (escopo desta feature)

- Nenhuma página dedicada ao Docker Manager no frontend
- Sem endpoints de gerenciamento (start/stop/restart, logs, volumes, networks, images)
- Navigation-drawer não possui submenu Docker
- Sem controller `docker_manager_controller.ts`
- Sem service de gerenciamento `docker_manager_service.ts`

---

## 🏗️ FASE 1 — Backend: Service de Gerenciamento

### 1.1 Expandir `DockerEngineHttpClient`

- [x] Adicionar método `postJson<T>(path, body?)` para ações POST
- [x] Adicionar método `deleteJson<T>(path)` para remoção
- [x] Adicionar método `getStream(path)` para streaming de logs
- [ ] Testes unitários do cliente HTTP expandido

### 1.2 Criar `DockerManagerService`

> Arquivo: `backend/app/services/docker_manager_service.ts`

**Containers:**

- [x] `listContainers()` — lista todos os containers (running + stopped) agrupados por label `com.docker.compose.project`
- [x] `inspectContainer(id)` — detalhes completos (Config, HostConfig, NetworkSettings, Mounts, State)
- [x] `startContainer(id)` — POST `/containers/{id}/start`
- [x] `stopContainer(id)` — POST `/containers/{id}/stop`
- [x] `restartContainer(id)` — POST `/containers/{id}/restart`
- [x] `getContainerLogs(id, options)` — GET `/containers/{id}/logs` com parâmetros `tail`, `since`, `follow`, `timestamps`

**Volumes:**

- [x] `listVolumes()` — GET `/volumes`
- [x] `inspectVolume(name)` — GET `/volumes/{name}`
- [x] `removeVolume(name, force?)` — DELETE `/volumes/{name}`

**Networks:**

- [x] `listNetworks()` — GET `/networks`
- [x] `inspectNetwork(id)` — GET `/networks/{id}`

**Imagens:**

- [x] `listImages()` — GET `/images/json`
- [x] `inspectImage(id)` — GET `/images/{id}/json`
- [x] `removeImage(id, force?)` — DELETE `/images/{id}`
- [x] `pruneImages()` — POST `/images/prune`

---

## 🏗️ FASE 2 — Backend: Tipos TypeScript

### 2.1 Expandir `docker_discovery_types.ts` (ou criar `docker_manager_types.ts`)

- [x] `DockerContainerSummary` — campos: id, names, image, state, status, labels, ports, created
- [x] `DockerContainerDetail` — campos completos do inspect (env, cmd, entrypoint, mounts, networks, restart policy, etc.)
- [x] `DockerContainerGroup` — agrupamento por projeto: `{ projectName: string, containers: DockerContainerSummary[] }`
- [x] `DockerVolumeSummary` e `DockerVolumeDetail`
- [x] `DockerNetworkSummary` e `DockerNetworkDetail`
- [x] `DockerImageSummary` e `DockerImageDetail`
- [x] `DockerLogEntry` — `{ timestamp: string, stream: 'stdout' | 'stderr', message: string }`
- [x] `DockerActionResult` — resposta padronizada de ações (start/stop/restart)

---

## 🏗️ FASE 3 — Backend: Controller e Rotas

### 3.1 Criar `DockerManagerController`

> Arquivo: `backend/app/controllers/docker_manager_controller.ts`

**Containers:**

- [x] `GET  /api/docker/containers` — lista containers agrupados por projeto
- [x] `GET  /api/docker/containers/:id` — detalhes do container
- [x] `POST /api/docker/containers/:id/start` — iniciar container
- [x] `POST /api/docker/containers/:id/stop` — parar container
- [x] `POST /api/docker/containers/:id/restart` — reiniciar container
- [x] `GET  /api/docker/containers/:id/logs` — logs do container (query: `tail`, `since`, `timestamps`)

**Volumes:**

- [x] `GET    /api/docker/volumes` — listar volumes
- [x] `GET    /api/docker/volumes/:name` — detalhes do volume
- [x] `DELETE /api/docker/volumes/:name` — remover volume

**Networks:**

- [x] `GET /api/docker/networks` — listar networks
- [x] `GET /api/docker/networks/:id` — detalhes da network

**Imagens:**

- [x] `GET    /api/docker/images` — listar imagens
- [x] `GET    /api/docker/images/:id` — detalhes da imagem
- [x] `DELETE /api/docker/images/:id` — remover imagem
- [x] `POST   /api/docker/images/prune` — remover imagens não usadas

### 3.2 Registrar rotas em `start/routes.ts`

- [x] Adicionar grupo `/api/docker` com middleware de autenticação
- [x] Aplicar rate limit `strict` nas ações destrutivas (stop, remove, prune)
- [x] Lazy load do `DockerManagerController`

---

## 🏗️ FASE 4 — Frontend: Tipos e Serviço API

### 4.1 Adicionar tipos em `frontend/src/types/api.ts`

- [x] `DockerContainerSummary`
- [x] `DockerContainerDetail`
- [x] `DockerContainerGroup`
- [x] `DockerVolumeSummary` / `DockerVolumeDetail`
- [x] `DockerNetworkSummary` / `DockerNetworkDetail`
- [x] `DockerImageSummary` / `DockerImageDetail`
- [x] `DockerLogEntry`
- [x] `DockerActionResult`

### 4.2 Criar `frontend/src/services/dockerService.ts`

- [x] `getContainerGroups()` — `GET /api/docker/containers`
- [x] `getContainerDetail(id)` — `GET /api/docker/containers/:id`
- [x] `startContainer(id)` — `POST /api/docker/containers/:id/start`
- [x] `stopContainer(id)` — `POST /api/docker/containers/:id/stop`
- [x] `restartContainer(id)` — `POST /api/docker/containers/:id/restart`
- [x] `getContainerLogs(id, params)` — `GET /api/docker/containers/:id/logs`
- [x] `getVolumes()` — `GET /api/docker/volumes`
- [x] `getVolumeDetail(name)` — `GET /api/docker/volumes/:name`
- [x] `removeVolume(name)` — `DELETE /api/docker/volumes/:name`
- [x] `getNetworks()` — `GET /api/docker/networks`
- [x] `getNetworkDetail(id)` — `GET /api/docker/networks/:id`
- [x] `getImages()` — `GET /api/docker/images`
- [x] `getImageDetail(id)` — `GET /api/docker/images/:id`
- [x] `removeImage(id)` — `DELETE /api/docker/images/:id`
- [x] `pruneImages()` — `POST /api/docker/images/prune`

---

## 🏗️ FASE 5 — Frontend: Componentes Base

### 5.1 Criar componentes em `frontend/src/components/docker/`

**Containers:**

- [x] `ContainerStatusChip.vue` — chip colorido com status (running=green, stopped=red, paused=orange, etc.)
- [x] `ContainerCard.vue` — card com nome, imagem, status, portas expostas, botões de ação (start/stop/restart)
- [x] `ContainerProjectGroup.vue` — agrupa ContainerCards por nome do projeto docker-compose com cabeçalho expansível
- [x] `ContainerEnvironmentTable.vue` — tabela de variáveis de ambiente (com opção de ocultar valores sensíveis)
- [x] `ContainerMountsTable.vue` — tabela de volumes montados (source, destination, mode)
- [x] `ContainerNetworkTable.vue` — tabela de networks conectadas (nome, IP, aliases)
- [x] `ContainerPortsTable.vue` — tabela de portas (host:container/protocol)
- [x] `ContainerLogsViewer.vue` — visualizador de logs com scroll automático, filtro por stream (stdout/stderr), botão de atualização

**Volumes:**

- [x] `VolumeCard.vue` — card com nome, driver, mountpoint, uso estimado, botão de remoção
- [x] `VolumeDetailDialog.vue` — modal/dialog com detalhes completos do volume

**Networks:**

- [x] `NetworkCard.vue` — card com nome, driver, scope, subnet, containers conectados
- [x] `NetworkDetailDialog.vue` — modal/dialog com detalhes da network e containers conectados

**Imagens:**

- [x] `ImageCard.vue` — card com tag, ID resumido, tamanho, data de criação, botão de remoção
- [x] `ImageDetailDialog.vue` — modal/dialog com layers, env, cmd, entrypoint

**Comuns:**

- [x] `DockerUnavailableBanner.vue` — banner informativo quando o socket Docker não está disponível
- [x] `DockerActionConfirmDialog.vue` — dialog de confirmação para ações destrutivas

---

## 🏗️ FASE 6 — Frontend: Páginas

### 6.1 Criar páginas em `frontend/src/pages/docker/`

- [x] `index.vue` — overview: cards de resumo (total containers running/stopped, volumes, networks, images)
- [x] `containers/index.vue` — lista de containers agrupados por projeto
- [x] `containers/[id].vue` — detalhes do container (tabs: Informações, Ambiente, Volumes, Networks, Logs)
- [x] `volumes/index.vue` — lista de volumes com busca e filtro
- [x] `networks/index.vue` — lista de networks com detalhes dos containers conectados
- [x] `images/index.vue` — lista de imagens com busca por tag, botão de prune

### 6.2 Funcionalidades por página

**`containers/index.vue`:**

- [x] Listar projetos como grupos expansíveis (v-expansion-panels)
- [x] Dentro de cada grupo: ContainerCards com status em tempo real
- [x] Ação global por projeto: reiniciar todos, parar todos
- [x] Chips de filtro: All / Running / Stopped
- [x] Auto-refresh configurável (polling ou SSE)
- [x] Indicador "Docker indisponível" se socket não acessível

**`containers/[id].vue`:**

- [x] Tab "Informações": imagem, status, criado em, restart policy, comando, entrypoint
- [x] Tab "Ambiente": variáveis de ambiente (com toggle para mostrar/ocultar valores)
- [x] Tab "Volumes": tabela de mounts
- [x] Tab "Redes": tabela de networks e IPs
- [x] Tab "Logs": ContainerLogsViewer com opções de tail (50/200/500/Tudo) e timestamps
- [x] Barra de ações: Start / Stop / Restart com confirmação
- [x] Breadcrumb: Docker > Containers > NomeContainer

**`volumes/index.vue`:**

- [x] Tabela com colunas: Nome, Driver, Mountpoint, labels
- [x] Botão "Remover" com confirmação (`DockerActionConfirmDialog`)
- [x] Busca por nome

**`networks/index.vue`:**

- [x] Tabela com colunas: Nome, Driver, Scope, Subnet, Containers
- [x] Expandir linha para ver containers conectados e seus IPs

**`images/index.vue`:**

- [x] Tabela com colunas: Repositório:Tag, ID, Tamanho, Criado em
- [x] Botão "Remover" com confirmação
- [x] Botão "Limpar imagens não usadas (prune)" com confirmação
- [x] Busca por tag/repositório

---

## 🏗️ FASE 7 — Frontend: Navegação

### 7.1 Atualizar `frontend/src/layouts/default.vue`

- [x] Adicionar seção Docker Manager com submenu expansível no navigation-drawer
- [x] Subitem: **Visão Geral** (`/docker`) — ícone `mdi-docker`
- [x] Subitem: **Containers** (`/docker/containers`) — ícone `mdi-cube-outline`
- [x] Subitem: **Volumes** (`/docker/volumes`) — ícone `mdi-database-outline`
- [x] Subitem: **Redes** (`/docker/networks`) — ícone `mdi-graph-outline`
- [x] Subitem: **Imagens** (`/docker/images`) — ícone `mdi-layers-outline`
- [x] Manter comportamento rail/drawer existente com o novo grupo
- [x] Adicionar badge/indicador visual quando Docker está indisponível

---

## 🏗️ FASE 8 — Qualidade e Testes

### 8.1 Backend — TypeScript e Lint

- [x] `pnpm typecheck` sem erros após todas as implementações do backend
- [x] `pnpm lint` sem erros no backend

### 8.2 Backend — Testes funcionais (Japa)

> Criar: `backend/tests/functional/docker_manager.spec.ts`

- [x] Teste: `GET /api/docker/containers` retorna 200 (mock socket disponível)
- [x] Teste: `GET /api/docker/containers` retorna lista vazia/indisponível (sem socket)
- [x] Teste: `GET /api/docker/containers/:id` retorna 404 para container inexistente
- [x] Teste: `POST /api/docker/containers/:id/start` retorna sucesso
- [x] Teste: `POST /api/docker/containers/:id/stop` retorna sucesso
- [x] Teste: `GET /api/docker/volumes` retorna 200
- [x] Teste: `GET /api/docker/networks` retorna 200
- [x] Teste: `GET /api/docker/images` retorna 200
- [x] Teste: todas as rotas Docker requerem autenticação (retornam 401 sem token)

### 8.3 Frontend — TypeScript

- [x] `pnpm typecheck` sem erros no frontend após implementação
- [x] `pnpm lint` sem erros no frontend

---

## 🏗️ FASE 9 — Ajustes Finais e Documentação

- [x] Verificar que o socket `/var/run/docker.sock` está mapeado no `docker-compose.yml`
- [x] Adicionar variável de ambiente `BACKEND_CONTAINER_ID` no `.env.example` (já existe no `DockerEnvironmentService`)
- [x] Adicionar seção "Docker Manager" no `README.md`
- [x] Atualizar `CHECKLIST.md` principal marcando a feature Docker Manager como concluída
- [x] Atualizar tabela de endpoints da API no `CHECKLIST.md` com as rotas `/api/docker/*`

---

## 📊 Resumo de Artefatos

| Camada | Arquivos novos | Arquivos modificados |
|---|---|---|
| Backend Services | `docker_manager_service.ts`, `docker_manager_types.ts` | `docker_engine_http_client.ts`, `docker_discovery_types.ts` |
| Backend Controllers | `docker_manager_controller.ts` | — |
| Backend Routes | — | `start/routes.ts` |
| Backend Tests | `tests/functional/docker_manager.spec.ts` | — |
| Frontend Types | — | `types/api.ts` |
| Frontend Services | `services/dockerService.ts` | — |
| Frontend Components | `components/docker/*.vue` (12 componentes) | — |
| Frontend Pages | `pages/docker/*.vue` (6 páginas) | — |
| Frontend Layout | — | `layouts/default.vue` |

---

## 🔢 Ordem de Execução Recomendada

```
1. FASE 1 → FASE 2   # Backend: Service + Tipos (fundação)
2. FASE 3            # Backend: Controller + Rotas
3. FASE 8.1 / 8.2   # Backend: typecheck + lint + testes
4. FASE 4            # Frontend: Tipos + Service API
5. FASE 5            # Frontend: Componentes base
6. FASE 6            # Frontend: Páginas
7. FASE 7            # Frontend: Navegação
8. FASE 8.3          # Frontend: typecheck + lint
9. FASE 9            # Ajustes finais
```

---

**Progresso:** ~85 / ~85 itens concluídos
**Última atualização:** 2026-04-12
