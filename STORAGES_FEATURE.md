# Armazenamentos — Especificação de Implementação

## Conceito

Evolução das *Destinos de Armazenamento* para um gerenciador completo de armazenamentos de objetos. O modelo `StorageDestination` é expandido (sem ruptura de compatibilidade) para suportar operações além de destino de backup: exploração de arquivos, cópia entre buckets e geração de arquivos compactados.

Todo destino de armazenamento existente (S3, GCS, Azure Blob, SFTP, local) é um **Armazenamento**. A UI consolida ambos os conceitos sob o menu "Armazenamentos".

---

## Decisões Técnicas

### Ferramenta de Cópia entre Storages: `rclone`

- Suporta S3/MinIO/Cloudflare R2, GCS, Azure Blob, SFTP e local com uma única CLI.
- `rclone` recebe credenciais via CLI flags (`--s3-access-key-id`, etc.) — nunca escrito em arquivo de config, seguro para processos filhos.
- Instalado no Dockerfile Alpine com `apk add rclone`.

### Exploração de Arquivos: SDKs Nativos

- S3/MinIO/R2: `@aws-sdk/client-s3` (já presente na stack S3).
- GCS: `@google-cloud/storage`.
- Azure Blob: `@azure/storage-blob`.
- SFTP: `ssh2-sftp-client` (já usado para backup).
- Local: `node:fs/promises`.

Essa abordagem evita spawnar um processo para listagens simples e dá resposta mais rápida no explorador.

### Arquivamento (Download do Bucket): Streaming com `archiver`

- `archiver` cria streams tar.gz sem buffer total em memória.
- Objetos são listados e então baixados sequencialmente via SDK nativo e piped para o archiver.
- O archiver é piped direto para a resposta HTTP (chunked transfer), sem arquivo temporário.

### Novos Tipos de Provider

Para distinguir MinIO e Cloudflare R2 de S3 puro na UI (endpoint e auth paths diferem), uma nova coluna `provider` (string) é adicionada sem alterar o `type` enum (manter backward compat). O `type` permanece `'s3'` para os três; o `provider` indica `'aws_s3' | 'minio' | 'cloudflare_r2'`.

---

## Backend

### 1. Migração

Nova migration (`6_extend_storage_destinations.ts`):

- Adiciona coluna `provider` (string, nullable) à tabela `storage_destinations`.  
  Provider válidos: `aws_s3`, `minio`, `cloudflare_r2`, `google_gcs`, `azure_blob`, `sftp`, `local`.  
  Para registros antigos, preenchida por migration com base no `type`.
- Nenhum campo removido — zero breaking change nos backups existentes.

### 2. Model: `StorageDestination`

Adições:

- Campo `provider: StorageProvider` (coluna `provider`).
- Tipo `StorageProvider = 'aws_s3' | 'minio' | 'cloudflare_r2' | 'google_gcs' | 'azure_blob' | 'sftp' | 'local'`.
- Método `getProviderLabel(): string` — retorno legível para UI.
- O tipo de config do `minio` e `r2` é igual ao `s3` (endpoint, accessKeyId, secretAccessKey, bucket, prefix); o `provider` indica a semântica.

### 3. Novos Serviços

#### `BucketExplorerService`

Responsabilidade única: listar objetos de um Storage em um caminho específico.

```
listObjects(storage, path, options): Promise<BucketObject[]>
  BucketObject { key, name, size, lastModified, isDirectory, etag? }

getObjectMetadata(storage, key): Promise<BucketObjectMetadata>

getPresignedUrl(storage, key, expiresIn): Promise<string>   // para S3/R2/MinIO
```

Implementações internas segregadas por interface `StorageExplorerAdapter`:
- `S3ExplorerAdapter` — cobre aws_s3, minio, r2 (todos usam `@aws-sdk/client-s3` com endpoint customizado).
- `GcsExplorerAdapter`
- `AzureExplorerAdapter`
- `SftpExplorerAdapter`
- `LocalExplorerAdapter`

O serviço resolve o adapter via factory pelo `provider` do storage.

#### `BucketCopyService`

Responsabilidade: orquestrar cópia entre dois Storages usando `rclone`.

```
copy(source, destination, options): Promise<CopyJobResult>
  options { sourcePath?, destinationPath?, dryRun?, deleteExtraneous? }
  CopyJobResult { filesTransferred, bytesTransferred, errors, duration }
```

- Monta flags rclone dinamicamente (sem arquivo de config).
- Spawna `rclone copy :source:path :dest:path [flags]` com `RCLONE_*` env vars por processo filho.
- Emite progresso em tempo real via SSE (canal `notifications/storage-copy`), mesmo padrão do `BackupProgressEmitter`.
- Redacta credenciais dos logs antes de emitir para auditoria.

#### `BucketArchiveService`

Responsabilidade: gerar um arquivo tar.gz de um Storage e streamar para o cliente.

```
createArchiveStream(storage, path?): Promise<Readable>
  // Retorna stream tar.gz que pode ser piped para response
```

- Lista todos os objetos recursivamente via `BucketExplorerService`.
- Baixa cada objeto sequencialmente e adiciona ao `archiver` (tar, gzip nível 6).
- O stream resultante é attached à resposta HTTP com headers corretos (`Content-Disposition`, `Content-Type: application/gzip`).
- Progresso estimado emitido via SSE usando total de arquivos listados vs. processados.

### 4. Novo Controller: `StoragesController`

Substitui visualmente o `StorageDestinationsController` para o usuário, mas ambos coexistem internamente. O novo controller delega CRUD ao model `StorageDestination` e adiciona as operações de bucket.

```
index()          — lista paginada com filtros (type, provider, status, search)
store()          — cria novo storage (valida provider + config)
show()           — detalhe + safeConfig
update()         — atualiza
destroy()        — remove (verifica backups vinculados)
test()           — testa conectividade (tenta listar raiz do bucket)
browse()         — GET /:id/browse?path=&cursor= → BucketObject[]
startCopy()      — POST /:id/copy { destinationId, sourcePath, destinationPath }
copyStatus()     — GET /copy-jobs/:jobId → status do job de cópia
startArchive()   — POST /:id/archive?path= → inicia geração (retorna jobId)
downloadArchive()— GET /archive-jobs/:jobId/download → stream tar.gz
```

**Validadores** novos: `createStorageValidator`, `updateStorageValidator`, `browseStorageValidator`, `copyStorageValidator`.

### 5. Rotas

Agrupadas sob `/api/storages`:

```
GET    /api/storages                          — index
POST   /api/storages                          — store
GET    /api/storages/:id                      — show
PUT    /api/storages/:id                      — update
DELETE /api/storages/:id                      — destroy

POST   /api/storages/:id/test                 — test connectivity (rate: strict)
GET    /api/storages/:id/browse               — file explorer
POST   /api/storages/:id/copy                 — start copy job (rate: backup)
GET    /api/storages/copy-jobs/:jobId         — copy job status
POST   /api/storages/:id/archive              — start archive job (rate: backup)
GET    /api/storages/archive-jobs/:jobId/download — stream archive
```

As rotas `/api/storage-destinations` existentes permanecem intactas (usadas pelo sistema de backup interno).

### 6. Jobs de Longa Duração (Copy / Archive)

Copy e archive são operações assíncronas. O fluxo é:

1. `POST /api/storages/:id/copy` retorna `202 Accepted` com `{ jobId }`.
2. SSE canal `notifications/storage-copy/:jobId` emite progresso.
3. `GET /api/storages/copy-jobs/:jobId` retorna estado atual.
4. Jobs são mantidos em memória por 24h (Map com TTL, mesmo padrão do restore).

Para archive, após conclusão, o stream fica disponível por 15min via download endpoint antes de expirar.

### 7. Docker

```dockerfile
# Adicionar ao Dockerfile existente
RUN apk add --no-cache rclone
```

Validar versão mínima `rclone v1.60+` no startup provider.

---

## Frontend

### Estrutura de Páginas (File-based routing)

```
src/pages/storages/
  index.vue                  — lista todos os armazenamentos
  new.vue                    — wizard de criação
  [id]/
    index.vue                — detalhe + edição do storage
    explore.vue              — explorador de arquivos
    copy.vue                 — wizard de cópia para outro storage
    download.vue             — página de archive/download
```

### Componentes

```
src/components/storages/
  StorageCard.vue            — card compacto com tipo, provider, status, ações
  StorageFormFields.vue      — campos dinâmicos por provider (S3, GCS, Azure, SFTP, local)
  StorageProviderIcon.vue    — ícone/logo por provider
  BucketExplorer.vue         — explorador de arquivos (tabela + breadcrumb + preview)
  BucketObjectRow.vue        — linha da tabela do explorador
  CopyJobProgress.vue        — progress card SSE para copy jobs
  ArchiveProgress.vue        — progress card SSE para archive jobs
  StorageTestButton.vue      — botão com estado de teste de conectividade
```

### Stores

#### `useStoragesStore`

```typescript
state: {
  storages: Storage[]
  pagination: PaginatedMeta
  loading: boolean
}

actions:
  fetchAll(filters?)
  create(payload)
  update(id, payload)
  remove(id)
  testConnection(id)
```

#### `useStorageExplorerStore`

```typescript
state: {
  currentStorage: Storage | null
  currentPath: string
  objects: BucketObject[]
  breadcrumbs: PathSegment[]
  cursor: string | null   // paginação por cursor (S3 continuationToken)
  loading: boolean
}

actions:
  browse(storageId, path?, cursor?)
  navigateTo(path)
  navigateUp()
  refresh()
```

#### `useStorageCopyStore`

```typescript
state: {
  activeJobs: Map<string, CopyJob>
}

actions:
  startCopy(sourceId, payload)
  subscribeToJob(jobId)    // SSE listener
  cancelJob(jobId)
```

### API Service (extensão de `api.ts`)

Novo grupo `storagesApi`:

```typescript
storagesApi.list(filters?)
storagesApi.get(id)
storagesApi.create(payload)
storagesApi.update(id, payload)
storagesApi.delete(id)
storagesApi.test(id)
storagesApi.browse(id, path?, cursor?)
storagesApi.startCopy(id, payload)
storagesApi.getCopyJob(jobId)
storagesApi.startArchive(id, path?)
storagesApi.downloadArchive(jobId)   // blob download com autenticação
```

### Tipos (extensão de `types/api.ts`)

```typescript
type StorageProvider = 'aws_s3' | 'minio' | 'cloudflare_r2' | 'google_gcs' | 'azure_blob' | 'sftp' | 'local'

interface Storage extends StorageDestination {
  provider: StorageProvider
}

interface BucketObject {
  key: string
  name: string
  size: number | null        // null para diretórios
  lastModified: string | null
  isDirectory: boolean
  etag?: string
}

interface CopyJob {
  id: string
  sourceStorageId: number
  destinationStorageId: number
  status: 'pending' | 'running' | 'completed' | 'failed'
  filesTransferred: number
  totalFiles: number | null
  bytesTransferred: number
  error?: string
  startedAt: string
  completedAt?: string
}

interface ArchiveJob {
  id: string
  storageId: number
  path: string | null
  status: 'pending' | 'building' | 'ready' | 'expired' | 'failed'
  totalFiles: number | null
  processedFiles: number
  downloadUrl?: string
  expiresAt?: string
}
```

### Navegação (default.vue)

O nav item "Armazenamentos" é um grupo expansível no drawer. Em rail mode, exibe somente o ícone com tooltip.

```typescript
{
  title: 'Armazenamentos',
  icon: 'mdi-bucket',
  to: '/storages',
  // Sem sub-itens fixos no drawer — ações contextuais são feitas dentro das páginas
}
```

Na página `storages/index.vue`, um FAB e um header com botão "Adicionar" contextualizam as ações.

As ações **Explorar**, **Copiar** e **Download** são acionadas por um menu de ações em cada `StorageCard` / linha da tabela, redirecionando para as sub-páginas de `/storages/:id/explore`, `/storages/:id/copy`, `/storages/:id/download`.

Isso mantém o drawer limpo e coloca as ações onde fazem sentido: no contexto do storage selecionado.

---

## UX — Fluxos Principais

### Criar Armazenamento

1. Usuário clica "Adicionar" na listagem.
2. Formulário em `/storages/new` com step 1: escolha do **provider** (cards visuais com logo).
3. Step 2: campos específicos do provider (região, bucket, endpoint, credenciais).
4. Step 3: botão "Testar Conexão" obrigatório antes de confirmar — validação real.
5. Opção "Definir como padrão para backups" (mantém compatibilidade com o sistema de backup).
6. Salvo → redireciona para `/storages/:id` com toast de confirmação.

### Explorar Bucket

1. Usuário acessa `/storages/:id/explore`.
2. Tabela com colunas: nome, tipo (arquivo/pasta), tamanho, última modificação.
3. Breadcrumb clicável para navegação.
4. Click em pasta: navega para o path.
5. Click em arquivo: abre menu com opções — Baixar arquivo individual (presigned URL) e Ver metadados.
6. Paginação por cursor (botão "Carregar mais").
7. Campo de busca por prefixo dentro do path atual.

### Copiar para Outro Bucket

1. Usuário acessa `/storages/:id/copy`.
2. Seleciona storage de destino (dropdown dos storages cadastrados).
3. Define path de origem (opcional) e path de destino (opcional).
4. Opções avançadas: dry-run, sobrescrever existentes, deletar extras no destino (sync mode).
5. Resumo antes de confirmar.
6. Após submit: card de progresso SSE em tempo real (arquivo atual, velocidade estimada, erros).
7. Histórico de jobs na página até expirar.

### Download como Arquivo Compactado

1. Usuário acessa `/storages/:id/download`.
2. Seleciona path raiz (opcional — padrão: bucket inteiro).
3. Estimativa de tamanho (listagem prévia).
4. Botão "Gerar Archive" — retorna jobId.
5. Progress bar SSE mostrando `processedFiles / totalFiles`.
6. Quando `status = ready`: botão "Baixar .tar.gz" fica disponível (válido por 15min).
7. Download autenticado via `Authorization: Bearer` no header.

---

## Integração com o Sistema de Backup Existente

- A Connection pode referenciar qualquer Storage como destino de backup (campo `storage_destination_id` — inalterado).
- No formulário de Connection, o campo "Destino de Backup" lista todos os Storages ativos.
- O `StorageDestinationService` existente não é alterado — continua funcionando para backups.
- Backups armazenados em um Storage podem ser explorados via BucketExplorer filtrando pelo prefix do backup.

---

## Segurança

- Credenciais nunca expostas nas respostas da API (`getSafeConfig()` inalterado, estendido para novos providers).
- rclone recebe credenciais via env vars no processo filho (não via arquivo de config gravado em disco).
- Jobs de cópia são validados: usuário autenticado deve ter acesso aos dois storages (source e destination pertencem ao sistema).
- Presigned URLs para download individual expiram em 15min (configurável).
- Logs de auditoria registram todas as operações de cópia e archive com usuário e IP.
- Archive jobs expiram automaticamente (TTL = 15min após `ready`).

---

## Ordem de Implementação

1. ~~**Migration** — coluna `provider` com backfill.~~ ✅
2. ~~**Model** — adicionar campo `provider` e `getProviderLabel()`.~~ ✅
3. ~~**BucketExplorerService** — adapters por provider (começar com S3/MinIO/R2, depois GCS, Azure).~~ ✅
4. ~~**StoragesController** — CRUD + `test()` + `browse()`.~~ ✅
5. ~~**Rotas** — `/api/storages/*`.~~ ✅
6. ~~**Frontend: tipos e API service** — extensões em `types/api.ts` e `services/api.ts`.~~ ✅
7. ~~**Frontend: StoragesStore + ExplorerStore**.~~ ✅
8. ~~**Frontend: páginas** — index → new → [id]/index → [id]/explore.~~ ✅
9. ~~**BucketCopyService + SSE** — CopyStore + páginas copy e download.~~ ✅
10. ~~**BucketArchiveService + streaming**.~~ ✅
11. **Integração com Connection form** — substituir dropdown de storage destinations.
12. ~~**Docker** — adicionar rclone ao Dockerfile.~~ ✅
13. **Testes** — unitários nos adapters, funcionais nas rotas.
