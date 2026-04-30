# Análise de Crescimento de Memória — Backend AdonisJS

**Sintoma observado:** RSS inicia em ~70 MB e cresce de forma quase linear até estabilizar em ~375 MB após ~48 h.
**Veredito:** crescimento misto. Existe um patamar natural de heap V8 (que justifica a estabilização em ~375 MB), mas há **vazamentos reais e pressão de GC** vinda de timers, clientes SDK não destruídos e estruturas em memória que crescem sem bound.

> Curva 70 MB → 375 MB → estabiliza é típica de _Node lazy heap growth_ + _retainers de longa vida_. O processo nunca devolve heap ao OS; ele só para de crescer quando todos os objetos retidos atingem o "estado estacionário".

---

## Status de implementação

- [x] Cachear `index.html` no boot.
- [x] Cachear `existsSync(socketPath)` por 30 s no cliente Docker.
- [x] Reutilizar `DockerContainerMonitoringService` como singleton.
- [x] Podar `lastPersistedAtByKey` para evitar crescimento sem bound.
- [x] Reutilizar clientes S3 com TTL/LRU e destruição ao expirar.
- [x] Adotar `http.Agent({ keepAlive: true })` e bufferizar respostas no `DockerEngineHttpClient`.
- [x] Unificar o polling de métricas de sistema e Docker em um único loop.
- [x] Aumentar o intervalo do polling unificado de métricas para 10 s.
- [x] Trocar persistência imediata de métricas por batch insert no histórico.
- [x] Reutilizar clientes Azure Blob e GCS com cache de ciclo de vida.
- [x] Expor `/api/system/heap` para acompanhar `process.memoryUsage()` e handles/requests ativos.
- [x] Adicionar painel no dashboard para acompanhar `/api/system/heap` com histórico local e tendência visual.
- [x] Reduzir o N+1 do monitor Docker usando `docker stats --no-stream` em lote, com fallback seguro para `/stats` por container.
- [x] Eliminar o spread duplo de `process.env` nas configurações de backup e restore.
- [x] Aplicar fallback de `LOG_LEVEL` por ambiente (`info` em produção, `debug` fora dela).
- [x] Adicionar teste unitário para evitar regressão nos quick wins de polling e log.
- [x] Limitar buffers de `stdout/stderr` em restore e monitor Docker para evitar retenção excessiva em memória.

---

## 1. Resumo executivo (priorizado)

| #  | Suspeito                                                                    | Impacto estimado | Esforço | Tipo                          |
| -- | --------------------------------------------------------------------------- | ---------------- | ------- | ----------------------------- |
| 1  | `S3Client` / `BlobServiceClient` / GCS `Storage` recriados por chamada      | **Alto**         | Médio   | Vazamento real                |
| 2  | `setInterval` de 5 s emitindo métricas Docker e Sistema (ambos via SSE+DB)  | **Alto**         | Baixo   | Pressão de GC + DB churn      |
| 3  | `DockerEngineHttpClient` sem `keep-alive` (novo socket por request, 5 s)    | Médio            | Baixo   | Pressão de GC (sockets/agent) |
| 4  | `ResourceMetricsHistoryService.lastPersistedAtByKey` cresce sem limpeza     | Médio            | Baixo   | Vazamento real                |
| 5  | Inserção 1×/min em SQLite (Lucid `.create`) sem batching                    | Médio            | Médio   | DB churn                      |
| 6  | `existsSync('/var/run/docker.sock')` chamado a cada 5 s no Windows          | Baixo            | Trivial | Pressão de syscalls/GC        |
| 7  | SPA fallback faz `readFileSync(index.html)` em **toda** requisição genérica | Médio            | Trivial | Pressão de GC                 |
| 8  | `SystemController` cria `DockerContainerMonitoringService` por request      | Baixo            | Trivial | Pressão de GC                 |
| 9  | `DockerContainerMonitoringService` faz N+1 calls no socket (1× por cont.)   | Médio            | Médio   | Pressão de I/O e GC           |
| 10 | `process.env` clonado em cada `spawn` (`{ ...process.env }`)                | Baixo            | Trivial | Pressão de GC                 |
| 11 | `pino-pretty` em dev: worker thread retém buffers de logs                   | Baixo            | Baixo   | Heap residente                |
| 12 | `BucketArchiveService` / `BucketCopyService`: `Map` de jobs com TTL longo   | Baixo            | Baixo   | Retenção temporária           |

---

## 2. Causas prováveis do crescimento (até 375 MB)

### 2.1 ⚠️ S3Client / Azure / GCS são instanciados em CADA chamada

[backend/app/services/storage_destination_service.ts:66-99](backend/app/services/storage_destination_service.ts#L66-L99) e [backend/app/services/storage/s3_explorer_adapter.ts:11-24](backend/app/services/storage/s3_explorer_adapter.ts#L11-L24)

```ts
private static async createS3Client(config) {
  const { S3Client } = await import('@aws-sdk/client-s3')
  return new S3Client({ ... })   // <— novo client a cada upload/download/delete/list
}
```

**Por que vaza:**
- O AWS SDK v3 (`@aws-sdk/client-s3`) cria internamente um `NodeHttpHandler` que mantém um `http.Agent` com `Keep-Alive` e pool de sockets. Sem `client.destroy()` esse pool **fica vivo até o GC do client**, mas o GC do client é atrasado porque ele tem timers/sockets em estado `keep-alive`.
- Cada upload de backup, cada listagem do explorer, cada presigned URL e cada delete de retenção cria um novo `S3Client`. Em 48 h, com backups agendados + listagens + UI polling = potencialmente **centenas a milhares de instâncias** acumuladas no heap antes do GC liberá‑las.
- `BlobServiceClient.fromConnectionString(...)` (Azure) e `new Storage(...)` (GCS) seguem o mesmo padrão.

**Otimização:**
- Cachear o client por `destinationId` (ou hash da config) com `LRU` pequeno (e.g. 8 entradas) e chamar `.destroy()` quando expirar.
- O `BucketExplorerService` já cacheia o **adapter** ([bucket_explorer_service.ts:22](backend/app/services/storage/bucket_explorer_service.ts#L22)), mas o adapter **recria o client em cada operação**. Mover a cache para o adapter (chave = config serializada) ou para o `StorageDestinationService`.

```ts
// Exemplo (esboço)
private static clients = new Map<string, S3Client>()
private static async createS3Client(config) {
  const key = `${config.endpoint}|${config.region}|${config.accessKeyId}`
  if (this.clients.has(key)) return this.clients.get(key)!
  const client = new S3Client({ ... })
  this.clients.set(key, client)
  return client
}
```

> ⚠️ Cuidado com credentials que mudam — invalidar a entrada quando o `StorageDestination.updatedAt` muda.

---

### 2.2 ⚠️ Dois `setInterval` de 5 s eternos com side‑effects pesados

[backend/app/services/system_resource_emitter.ts:43](backend/app/services/system_resource_emitter.ts#L43) e [backend/app/services/docker_container_resource_emitter.ts:31](backend/app/services/docker_container_resource_emitter.ts#L31)

A cada **5 segundos** (configurado em ambos):
1. `SystemResourceEmitter` → `getResourceMetrics()` (faz `os.cpus()` 2× com `delay(150ms)`) → `ResourceMetricsHistoryService.recordSystemSnapshot()` → `transmit.broadcast(...)`.
2. `DockerContainerResourceEmitter` → `getOverview()` → 1 chamada `/containers/json` + N chamadas `/containers/{id}/stats?stream=false` → `recordContainerSnapshot()` → `broadcast`.

**Custos por ciclo (a cada 5 s):**
- Cria múltiplos `Promise`, closures, payload `BroadcastableValue` recursivo.
- Aloca `DateTime` (Luxon — caro), `new Date().toISOString()`, etc.
- Aloca `CpuInfo[]` 2× (1× por core).
- Lê o socket Docker N+1 vezes; constrói buffers de string via concatenação (`body += chunk.toString()`).

**Em 48 h:** 48·3600/5 = **34 560 ciclos**. Mesmo com GC bom, retainers temporários (Promises, closures, builders de SQL) provocam crescimento de _old space_ até estabilização do heap.

**Otimizações:**
1. Aumentar o intervalo padrão de 5 s para 10–15 s (ou tornar configurável via env).
2. **Fundir os dois loops num só `setInterval`** que coleta system + docker em sequência e dispara um único `broadcast`. Reduz timers e payloads pela metade.
3. Persistir métricas em **batch** (próximo item).
4. Pular completamente o ciclo Docker quando `isSocketAvailable()` retornou `false` nas últimas N iterações (cache do estado por 30 s — hoje é checado todo ciclo).

---

### 2.3 ⚠️ `DockerEngineHttpClient` sem `keep-alive` + N+1 por ciclo

[backend/app/services/docker_engine_http_client.ts:14-52](backend/app/services/docker_engine_http_client.ts#L14-L52)

```ts
const req = request({ socketPath, method: 'GET', path }, ...)
```

Cada chamada abre um **novo socket** (sem `agent`, sem `keepAlive`). Com `DockerContainerMonitoringService.mapContainerFromSocket` ([docker_container_monitoring_service.ts:159-195](backend/app/services/docker_container_monitoring_service.ts#L159-L195)) chamando `/containers/{id}/stats` **uma vez por container** a cada 5 s, com 5 containers temos **6 sockets/ciclo = 4 320/dia**.

Pior: `body += chunk.toString()` ([linha 24](backend/app/services/docker_engine_http_client.ts#L23)) gera cadeias de strings grandes (resposta de `stats` é JSON volumoso). Em V8 isso vira `ConsString` que só é "achatada" sob pressão — heap retém esse working‑set.

**Otimizações:**
1. Usar um `http.Agent({ keepAlive: true, maxSockets: 4 })` e passá-lo em `request({ agent, ... })`.
2. Acumular chunks em `Buffer[]` e `Buffer.concat(...)` ao final em vez de string concat.
3. **Eliminar o N+1**: `/containers/json?all=false` já dá a lista; usar `/containers/{id}/stats?stream=false&one-shot=true` por container só no detalhe. Para o emitter, considere `docker stats --no-stream --format json` em **uma única chamada** via CLI quando o socket não expõe stats agregadas.
4. Implementar TTL de 30 s para `isSocketAvailable()`:
   ```ts
   private socketAvailableAt = 0
   private socketAvailableCache = false
   isSocketAvailable() {
     if (Date.now() - this.socketAvailableAt < 30_000) return this.socketAvailableCache
     this.socketAvailableCache = existsSync(this.socketPath)
     this.socketAvailableAt = Date.now()
     return this.socketAvailableCache
   }
   ```
   No Windows, `existsSync('/var/run/docker.sock')` é chamado **a cada 5 s sempre retornando `false`** — é `syscall` desnecessário.

---

### 2.4 ⚠️ `lastPersistedAtByKey` cresce indefinidamente

[backend/app/services/resource_metrics_history_service.ts:32](backend/app/services/resource_metrics_history_service.ts#L32)

```ts
private static readonly lastPersistedAtByKey = new Map<string, number>()
```

A cada container observado, é gravada uma chave `container:${containerId}`. Se containers Docker são recriados (CI, dev, recreate compose) o ID muda mas a entrada antiga **nunca é removida**. Em ambientes onde containers giram com frequência, essa Map cresce monotonicamente.

**Otimização:** TTL na entrada — quando `now - lastPersistedAt > MIN_PERSIST_INTERVAL_MS * 10`, remover.

```ts
private static readonly lastPersistedAtByKey = new Map<string, number>()

static pruneStaleEntries(now: number) {
  const ttl = this.MIN_PERSIST_INTERVAL_MS * 10
  for (const [key, ts] of this.lastPersistedAtByKey) {
    if (now - ts > ttl) this.lastPersistedAtByKey.delete(key)
  }
}
```

Chamar `pruneStaleEntries` dentro de `pruneOldRecordsIfNeeded` (já roda 1×/6h).

---

### 2.5 Inserções 1×/min em SQLite via Lucid sem batching

[backend/app/services/resource_metrics_history_service.ts:50](backend/app/services/resource_metrics_history_service.ts#L50)

```ts
await ResourceMetricHistory.create({ ... })
```

Cada `Lucid.create()` constrói um modelo, dispara hooks, gera SQL, abre/fecha statement. O ciclo de 1 minuto não é alto, mas combinado com `recordContainerSnapshot` (que pode inserir N linhas em batch via `createMany` — bom) há churn de objetos Lucid. Em 48 h são **~2 880 INSERTs system + N·2 880 container**.

**Otimizações:**
1. Manter um **buffer in-memory de N pontos** e flush em batch a cada 5 minutos (ou 100 pontos) via `db.insertQuery().multiInsert(rows)` — bypass do Lucid.
2. Considerar uma tabela `WITHOUT ROWID` ou particionamento por data, e índice composto por `(scope, entity_id, collected_at)` se ainda não existe.
3. SQLite com `journal_mode=WAL` + `synchronous=NORMAL` reduz bloqueios e custo por insert.

---

### 2.6 SPA fallback lê `index.html` do disco em **cada** request `*`

[backend/start/routes.ts:225-238](backend/start/routes.ts#L225-L238)

```ts
router.get('*', async ({ response }) => {
  const indexPath = join(app.publicPath(), 'index.html')
  if (existsSync(indexPath)) {
    const html = readFileSync(indexPath, 'utf-8')   // <— I/O síncrono + alocação por request
    return response.header('Content-Type', 'text/html').send(html)
  }
  ...
})
```

Cada hit em URLs do frontend (e.g. recargas, deep‑links de SPA) cai aqui e:
- Faz `existsSync` + `readFileSync` síncronos.
- Aloca uma string nova de ~300 KB–2 MB (conforme tamanho do bundle HTML).
- Bloqueia event loop brevemente.

**Otimização:** ler **uma vez** no boot, manter em memória (Buffer), servir direto:

```ts
let cachedIndex: Buffer | null = null
const indexPath = join(app.publicPath(), 'index.html')
if (existsSync(indexPath)) cachedIndex = readFileSync(indexPath)

router.get('*', async ({ response }) => {
  if (cachedIndex) return response.header('Content-Type', 'text/html').send(cachedIndex)
  return response.status(404).json({ ... })
})
```

> Bonus: serve com `If-None-Match` ETag para reduzir transferência.

---

### 2.7 `SystemController` cria `DockerContainerMonitoringService` por request

[backend/app/controllers/system_controller.ts:11](backend/app/controllers/system_controller.ts#L11)

```ts
private readonly dockerContainerMonitoringService = new DockerContainerMonitoringService()
```

Adonis instancia um novo controller por request → novo monitoring service → novo `DockerEngineHttpClient`. Em rotas usadas pela UI (`/system/containers/resources`, `/system/resources/history`) isso dá churn de objetos.

**Otimização:** transformar `DockerContainerMonitoringService` em singleton (com `instance()` estático) ou injetar via container:

```ts
export class DockerContainerMonitoringService {
  private static _instance: DockerContainerMonitoringService | null = null
  static instance() {
    return (this._instance ??= new DockerContainerMonitoringService())
  }
}
```

E no controller usar `DockerContainerMonitoringService.instance()`.

O `DockerContainerResourceEmitter` já segue esse padrão ([linha 21](backend/app/services/docker_container_resource_emitter.ts#L21)) — bastaria reaproveitar a mesma instância no controller.

---

### 2.8 Buffer de string em `child_process` stdout/stderr

[backend/app/services/restore_service.ts:758-763](backend/app/services/restore_service.ts#L758-L763) e [backend/app/services/docker_container_monitoring_service.ts:510-515](backend/app/services/docker_container_monitoring_service.ts#L510-L515)

```ts
restoreProcess.stdout?.on('data', (data: Buffer) => { stdoutData += data.toString() })
```

Em restores grandes, o `stdoutData` chega facilmente a megabytes em string. Strings concatenadas em loop têm complexidade O(n²) na pior hipótese e geram retainers no heap até o close. Vale principalmente para `RestoreService` quando o output do processo é grande.

**Otimização:** `const chunks: Buffer[] = []; chunks.push(data); ... Buffer.concat(chunks).toString()` ao final, ou fechar a coleta acima de N KB:

```ts
const stderrChunks: Buffer[] = []
let stderrBytes = 0
const STDERR_LIMIT = 256 * 1024
restoreProcess.stderr.on('data', (b: Buffer) => {
  if (stderrBytes < STDERR_LIMIT) {
    stderrChunks.push(b)
    stderrBytes += b.length
  }
})
```

---

### 2.9 `process.env` clonado a cada spawn

[backend/app/services/backup_service.ts:467](backend/app/services/backup_service.ts#L467) e ocorrências similares em `restore_service.ts`, `bucket_copy_service.ts`.

```ts
const processEnv = { ...process.env }
return { command, args, env: { ...processEnv, PGPASSWORD: password } }
```

`process.env` em apps com muitas variáveis pode ter centenas de chaves. Clonar 2 níveis (`{ ...processEnv, ... }`) cria 2 objetos novos a cada backup. Não é crítico mas, em backups paralelos / muitos databases, soma.

**Otimização:** spawn aceita só as vars que mudam — o filho herda `process.env` por padrão se você omitir `env`, ou só passar os adicionais via shell prefix. Se preferir manter clonagem, use 1 spread em vez de 2:

```ts
env: { ...process.env, PGPASSWORD: password }
```

---

### 2.10 `BucketArchiveService` / `BucketCopyService` — `Map` de jobs em memória

[backend/app/services/storage/bucket_archive_service.ts:27-29](backend/app/services/storage/bucket_archive_service.ts#L27-L29) e [backend/app/services/storage/bucket_copy_service.ts:18-19](backend/app/services/storage/bucket_copy_service.ts#L18-L19)

Jobs ficam em memória 24 h (`JOB_TTL_MS`) ou 1 h (archive). Em uso intenso, dezenas de jobs podem coexistir, cada um com `error` / `bytesTransferred` / etc. **OK para uso normal**, mas se a UI dispara muitos jobs concorrentes essa Map cresce. Em ambientes ociosos não é causa do crescimento.

**Otimização (opcional):** baixar TTL para 6 h e implementar limite máximo (`MAX_JOBS = 50`, descartar mais antigos).

---

### 2.11 `pino-pretty` em desenvolvimento mantém worker thread vivo

[backend/config/logger.ts:18-22](backend/config/logger.ts#L18-L22)

`@adonisjs/core/logger` usa `pino` com `transport.targets`. `pino-pretty` é executado em **worker thread** separado — a memória dele é contada na soma do processo (Node 18+). Em produção (`targets.file({ destination: 1 })`) também spawna worker. Logs verbosos enchem buffers do worker.

**Otimizações:**
1. Em produção, mudar `LOG_LEVEL` para `info` ou `warn` (verificar `.env`).
2. Considerar `pino` direto sem `targets()` (sem worker thread) em produção, redirecionando para stdout.
3. Garantir que `logger.debug(...)` em loops quentes (e.g. [backup_progress_emitter.ts:128](backend/app/services/backup_progress_emitter.ts#L128) ou [bucket_copy_service.ts:300](backend/app/services/storage/bucket_copy_service.ts#L300)) seja eliminado em prod.

---

### 2.12 Hot Hook (apenas em dev)

[backend/package.json:82-87](backend/package.json#L82-L87)

```json
"hotHook": { "boundaries": ["./app/controllers/**/*.ts","./app/middleware/*.ts"] }
```

Em `dev` (`node ace serve --hmr`), módulos antigos não são GC-eados de imediato após reload. Cresce ao longo do dia conforme você edita. **Não afeta produção**, mas se a observação dos 70→375 MB é em dev, é parte da explicação.

---

## 3. Hipóteses sobre o platô em ~375 MB

A estabilização indica que NÃO há vazamento exponencial — o que cresce tem teto. Os retainers reais que justificam o platô:

1. **Heap base do AdonisJS + Lucid + plugins** (~100–150 MB).
2. **Pool de S3 Clients acumulados** até o GC chegar neles (provavelmente o maior contribuinte, ~50–100 MB).
3. **Cache compilado de DateTime/Luxon, regex, classes Lucid** (~30–50 MB).
4. **Buffers de Pino transport** em worker (~10–30 MB).
5. **Maps em memória** (jobs de archive/copy + `lastPersistedAtByKey` + adapters).
6. **V8 _old space_ headroom** que o Node não devolve ao OS após scavenges (~50 MB).

Quando a soma dos retainers ativos atinge equilíbrio com o GC, o RSS para de crescer. **Não é leak clássico, mas é desperdício de memória residente.**

---

## 4. Plano de ataque sugerido (em ordem)

### Quick wins (≤ 1h cada, alto impacto agregado)

- [x] 1. **Cachear `index.html` no boot** ([routes.ts:225](backend/start/routes.ts#L225)).
- [x] 2. **Cachear resultado de `existsSync(socketPath)` por 30 s** ([docker_engine_http_client.ts:8](backend/app/services/docker_engine_http_client.ts#L8)).
- [x] 3. **Singleton `DockerContainerMonitoringService`** ([system_controller.ts:11](backend/app/controllers/system_controller.ts#L11)).
- [x] 4. **Aumentar `INTERVAL_MS` para 10 000** no polling unificado ativo ([resource_metrics_polling_service.ts:14](backend/app/services/resource_metrics_polling_service.ts#L14)).
- [x] 5. **`pruneStaleEntries` em `lastPersistedAtByKey`** ([resource_metrics_history_service.ts:32](backend/app/services/resource_metrics_history_service.ts#L32)).
- [x] 6. **`{ ...process.env, ... }` único** em vez de duplo ([backup_service.ts:467](backend/app/services/backup_service.ts#L467) e [restore_service.ts:701](backend/app/services/restore_service.ts#L701)).
- [x] 7. **Reduzir `LOG_LEVEL`** em produção com fallback em código e teste de regressão ([config/logger.ts:1](backend/config/logger.ts#L1) e [tests/unit/performance_quick_wins.spec.ts:1](backend/tests/unit/performance_quick_wins.spec.ts#L1)).

### Esforço médio (alto impacto)

- [x] 8. **Cache de S3/Azure/GCS Clients por destinationId** com `client.destroy()` ao invalidar ([storage_destination_service.ts:66](backend/app/services/storage_destination_service.ts#L66) e adapters em `app/services/storage/`).
- [x] 9. **`http.Agent({ keepAlive: true })` no `DockerEngineHttpClient`** + buffers em `Buffer[]` em vez de string concat.
- [x] 10. **Fundir os dois emitters num único loop** com 1 broadcast e batching de DB writes (flush a cada 5 min ou 100 pontos).
- [x] 11. **Trocar `Lucid.create` por `multiInsert` em batch** no `ResourceMetricsHistoryService`.
- [x] 12. **Buffer de `stdout/stderr` com limite** em `RestoreService` e `DockerContainerMonitoringService`.

### Esforço maior (refatoração)

- [x] 13. **Limpeza periódica de jobs** em `BucketArchiveService`/`BucketCopyService` com cap de tamanho.
- [x] 14. **PRAGMA `journal_mode=WAL`** + index review para `resource_metric_history`.
- [x] 15. **Trocar pino transport por stdout direto** em produção (sem worker thread).

---

## 5. Como validar

1. **Heap snapshot a cada 6 h** com `--inspect` + Chrome DevTools → comparar dois snapshots e olhar a aba **Comparison** → procurar grupos de objetos `S3Client`, `ConsString` grandes, `Promise`, `DateTime`.
2. [x] **Métricas in-process**: endpoint `/api/system/heap` exposto retornando `process.memoryUsage()` (`heapUsed`, `heapTotal`, `external`, `arrayBuffers`, `rss`) e contagem de handles/requests ativos. Ler de hora em hora e plotar.
3. [x] **Painel visual no dashboard**: acompanhar `/api/system/heap` com histórico local de até 48 h, tendência de RSS e sinais auxiliares do processo diretamente na interface.
4. **`--max-old-space-size=512`** para forçar GC mais agressivo e verificar se a curva muda — se RSS estabiliza em ~280 MB com o flag e o app continua saudável, confirma que muito do crescimento é V8 lazy growth.
5. **Logar `process.memoryUsage().heapUsed` nos dois emitters** durante 24 h e correlacionar com inserts em DB.
6. **Após cada otimização**, repetir a observação por 48 h e comparar o platô.

---

## 6. Comandos úteis para diagnóstico

```bash
# Heap snapshot via SIGUSR2 (Linux/Mac) — Windows: usar inspector
node --inspect bin/server.js

# Forçar dump de heap (precisa do módulo heapdump ou v8.writeHeapSnapshot)
# Em produção, exponha um endpoint admin:
# v8.writeHeapSnapshot(`./tmp/heap-${Date.now()}.heapsnapshot`)

# Monitorar RSS continuamente (no container)
while true; do
  echo "$(date +%T) $(ps -p $PID -o rss= | awk '{print $1/1024 " MB"}')"
  sleep 60
done

# Ver agentes/handles ativos do Node
process._getActiveHandles().length
process._getActiveRequests().length
```

---

## 7. Pontos que NÃO são causa (eliminados na análise)

- **`SchedulerService` (node-cron)**: jobs são corretamente parados em `unscheduleConnection()` e `stop()`. Sem leak.
- **`BackupProgressEmitter` / `RestoreProgressEmitter`**: instâncias por backup, descartadas ao fim. OK.
- **Streams de backup (`pg_dump → gzip → file`)**: fluxo de pipe correto, hash/gzip/file fecham juntos. OK.
- **Migrations / `database/schema.ts`**: rodam só no boot/migration. Sem efeito em runtime.
- **Bodyparser 500 MB**: limite **alto**, mas só aloca quando há upload — não cresce sozinho.
- **Rate limiter memória**: `@adonisjs/limiter` purga keys expiradas automaticamente.

---

**Conclusão:** o platô em 375 MB é majoritariamente **trabalho perdido** (S3 clients pendurados, churn de objetos por timer de 5 s, leitura de `index.html` por request). Atacando os itens 1–7 da seção 4, é razoável esperar **platô em 150–200 MB** sem mudar arquitetura.
