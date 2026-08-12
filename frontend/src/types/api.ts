/** Tipos locais de payload e reexports do contrato gerado no backend. */

import type { Connection as ConnectionDto } from '@/bindings/Connection'
import type { AuditLog as AuditLogDto } from '@/bindings/AuditLog'
import type { AuditStats as AuditStatsDto } from '@/bindings/AuditStats'
import type { Backup as BackupDto } from '@/bindings/Backup'
import type { BackupConnection as BackupConnectionDto } from '@/bindings/BackupConnection'
import type { BackupRetentionPolicy as BackupRetentionPolicyDto } from '@/bindings/BackupRetentionPolicy'
import type { ConnectionTestResult as ConnectionTestResultDto } from '@/bindings/ConnectionTestResult'
import type { Cpu as CpuDto } from '@/bindings/Cpu'
import type { ImportedBackup as ImportedBackupDto } from '@/bindings/ImportedBackup'
import type { Jobs as JobsDto } from '@/bindings/Jobs'
import type { MemorySource as MemorySourceDto } from '@/bindings/MemorySource'
import type { StorageDestination as StorageDestinationDto } from '@/bindings/StorageDestination'
import type { StorageDestinationDetail as StorageDestinationDetailDto } from '@/bindings/StorageDestinationDetail'
import type { Storage as StorageDto } from '@/bindings/Storage'
import type { StorageDetail as StorageDetailDto } from '@/bindings/StorageDetail'
import type { StorageSpace as StorageSpaceDto } from '@/bindings/StorageSpace'
import type { SystemOverview as SystemOverviewDto } from '@/bindings/SystemOverview'

// Status de uma conexão
export type ConnectionStatus = 'active' | 'inactive' | 'error'

// Tipos de banco de dados suportados
export type DatabaseType = ConnectionDto['type']

// Frequências de agendamento
export type ScheduleFrequency = '1h' | '6h' | '12h' | '24h'

// Status de um backup
export type BackupStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'

export type StorageDestinationType = 'local' | 's3' | 'gcs' | 'azure_blob' | 'sftp'

export type StorageDestinationStatus = 'active' | 'inactive'

// Tipo de retenção
export type RetentionType = 'hourly' | 'daily' | 'weekly' | 'monthly' | 'yearly'

// Trigger do backup
export type BackupTrigger = 'scheduled' | 'manual'

/**
 * Database associado a uma conexão
 */
export type Connection = ConnectionDto
export type { ConnectionDatabase } from '@/bindings/ConnectionDatabase'
export type { ConnectionListItem } from '@/bindings/ConnectionListItem'

/**
 * Dados para criação de uma conexão
 */
export interface CreateConnectionPayload {
  name: string
  type: DatabaseType
  host: string
  port: number
  databases: string[]
  username: string
  password: string
  storageDestinationId?: number | null
  scheduleFrequency?: ScheduleFrequency
  scheduleEnabled?: boolean
  options?: {
    ssl?: boolean
    charset?: string
  }
}

/**
 * Dados para atualização de uma conexão
 */
export interface UpdateConnectionPayload {
  name?: string
  type?: DatabaseType
  host?: string
  port?: number
  databases?: string[]
  username?: string
  password?: string
  storageDestinationId?: number | null
  scheduleFrequency?: ScheduleFrequency | null
  scheduleEnabled?: boolean
  options?: {
    ssl?: boolean
    charset?: string
  } | null
}

export type { HostResolutionSource as DockerHostResolutionSource } from '@/bindings/HostResolutionSource'
export type { PortOption as DockerPortOption } from '@/bindings/PortOption'
export type { HostSuggestion as DockerHostSuggestion } from '@/bindings/HostSuggestion'
export type { DockerHosts as DockerHostsResponseData } from '@/bindings/DockerHosts'

export type StorageDestinationConfigPayload =
  | {
      basePath?: string
    }
  | {
      region?: string
      bucket: string
      endpoint?: string
      accessKeyId: string
      secretAccessKey: string
      forcePathStyle?: boolean
      prefix?: string
    }
  | {
      bucket: string
      projectId?: string
      credentialsJson?: string
      usingUniformAcl?: boolean
      prefix?: string
    }
  | {
      connectionString: string
      container: string
      prefix?: string
    }
  | {
      host: string
      port?: number
      username: string
      password?: string
      privateKey?: string
      passphrase?: string
      basePath?: string
    }

export type StorageDestination = Omit<StorageDestinationDto, 'type' | 'status'> & {
  type: StorageDestinationType
  status: StorageDestinationStatus
  config?: Record<string, unknown> | null
}
export type StorageDestinationDetail = StorageDestinationDetailDto

export interface CreateStorageDestinationPayload {
  name: string
  type: StorageDestinationType
  status?: StorageDestinationStatus
  isDefault?: boolean
  config?: StorageDestinationConfigPayload
}

export interface UpdateStorageDestinationPayload {
  name?: string
  type?: StorageDestinationType
  status?: StorageDestinationStatus
  isDefault?: boolean
  config?: StorageDestinationConfigPayload
}

/**
 * Payload de Login
 */
export interface LoginPayload {
  email: string
  password: string
}

/**
 * Payload de Registro
 */
export interface RegisterPayload {
  fullName?: string
  email: string
  password: string
  bootstrapToken?: string
}

/**
 * Interface de um backup
 */
export type Backup = BackupDto

/**
 * Modo de restauração
 */
export type RestoreMode = 'full' | 'schema-only' | 'data-only'

/**
 * Opções para restauração de backup
 */
export interface RestoreOptions {
  mode?: RestoreMode
  /** ID da conexão de destino (se diferente da conexão original do backup) */
  targetConnectionId?: number
  targetDatabase?: string
  noOwner?: boolean
  noPrivileges?: boolean
  noTablespaces?: boolean
  noComments?: boolean
  noCreateDb?: boolean
  skipSafetyBackup?: boolean
  /** Limpar o banco de destino antes de restaurar */
  clearBeforeRestore?: boolean
}

/**
 * Resultado de uma restauração
 */
export interface RestoreResult {
  databaseName: string
  durationSeconds: number
  warnings?: string[]
  safetyBackup?: {
    id: number
    fileName: string | null
    fileSize: number | null
    success: boolean
  }
}

/**
 * Resumo de um backup (para listagens)
 */
export type { ConnectionBackupSummary as BackupSummary } from '@/bindings/ConnectionBackupSummary'

/**
 * Resumo de uma conexão (para listagens)
 */
export type ConnectionSummary = BackupConnectionDto

/**
 * As formas comuns a toda resposta vêm do backend, geradas por `ts-rs`.
 *
 * Redigitá-las aqui é o que fazia o frontend divergir em silêncio: um campo
 * renomeado no Rust virava `undefined` na tela em vez de erro de compilação.
 * Regerar: `cargo test --lib dtos` no backend.
 */
export type { ApiErrorBody } from '@/bindings/ApiErrorBody'
export type { FieldError } from '@/bindings/FieldError'
export type { MessageResponse } from '@/bindings/MessageResponse'
export type { PageInfo } from '@/bindings/PageInfo'
export type { Paginated } from '@/bindings/Paginated' 

/**
 * Informações de espaço de armazenamento
 */
export type StorageSpaceInfo = StorageSpaceDto

export type JobsSystemStatus = JobsDto

export type CpuResourceMetrics = CpuDto

export type MemoryMetricsSource = MemorySourceDto

export interface MemoryResourceMetrics {
  totalBytes: number
  usedBytes: number
  freeBytes: number
  usagePercent: number
  /** Origem do número: cgroup (dentro de container) ou os.* (fora). */
  source: MemoryMetricsSource
  /** `true` quando há limite de container efetivo aplicado. */
  containerLimited: boolean
}

export type { Resources as SystemResourceMetrics } from '@/bindings/Resources'

export type SystemStatus = SystemOverviewDto

export interface UpdateBackupRetentionPolicyPayload {
  daily: number
  weekly: number
  monthly: number
  yearly: number
  pruneCron: string
}

export type BackupRetentionPolicySettings = BackupRetentionPolicyDto

export interface DeletedBackupSummary {
  id: number
  connectionId: number | null
  connectionDatabaseId: number | null
  databaseName: string
  fileName: string | null
  retentionType: RetentionType
  createdAt: string | null
}

export interface BackupRetentionRunResult {
  deleted: number
  promoted: number
  protected: number
  errors: string[]
  deletedBackups: DeletedBackupSummary[]
}

export interface DiagnosticFile {
  name: string
  sizeBytes: number
  createdAt: string
  modifiedAt: string
}

export interface DiagnosticsListing {
  directory: string
  directoryExists: boolean
  files: DiagnosticFile[]
}

export interface DockerContainerResourceMetrics {
  containerId: string
  containerName: string
  projectName: string | null
  imageName: string
  status: string
  cpu: {
    usagePercent: number
  }
  memory: {
    usageBytes: number
    limitBytes: number
    usagePercent: number
  }
  network: {
    rxBytes: number
    txBytes: number
  }
  blockIo: {
    readBytes: number
    writeBytes: number
  }
  pids: number | null
}

export interface DockerContainerResourceOverview {
  dockerAvailable: boolean
  unavailableReason: string | null
  collectedAt: string
  containers: DockerContainerResourceMetrics[]
}

export interface ResourceHistoryPoint {
  timestamp: string
  cpuUsagePercent: number
  memoryUsagePercent: number
  memoryUsedBytes: number
  memoryTotalBytes: number
}

export interface ContainerResourceHistory {
  containerId: string
  containerName: string
  points: ResourceHistoryPoint[]
}

export interface ResourceMetricsHistoryResponse {
  retentionDays: number
  system: ResourceHistoryPoint[]
  containers: ContainerResourceHistory[]
}

export interface DashboardStats {
  connections: {
    total: number
    active: number
  }
  backups: {
    total: number
    today: number
  }
  recentBackups: {
    id: number
    connectionName: string
    status: BackupStatus
    fileSize: number | null
    createdAt: string
  }[]
  storageSpaces: StorageSpaceInfo[]
  system: SystemStatus
}

/**
 * Resultado de teste de conexão
 */
export type ConnectionTestResult = ConnectionTestResultDto

/**
 * Resultado de backup manual
 */
export interface BackupResult {
  backupId: number
  fileName: string
  fileSize: string
  duration: string
  checksum: string
}

/**
 * Formatos de arquivo de backup suportados para importação
 */
export type ImportedFileFormat = 'sql' | 'sql.gz' | 'dump' | 'zip' | 'tar'

/**
 * Resultado de verificação de integridade de um arquivo importado
 */
export interface IntegrityCheckResult {
  valid: boolean
  message: string
  warnings?: string[]
}

/**
 * Resultado da importação de um arquivo de backup
 */
export type ImportBackupResult = ImportedBackupDto

// ==================== Audit Logs ====================

/**
 * Ações auditáveis no sistema
 */
export type AuditAction
  = | 'connection.created'
    | 'connection.updated'
    | 'connection.deleted'
    | 'connection.tested'
    | 'backup.started'
    | 'backup.completed'
    | 'backup.failed'
    | 'backup.deleted'
    | 'backup.downloaded'
    | 'backup.imported'
    | 'settings.updated'
    | 'diagnostics.downloaded'
    | 'diagnostics.deleted'

/**
 * Tipos de entidades auditáveis
 */
export type AuditEntityType = 'connection' | 'backup' | 'settings' | 'diagnostics'

/**
 * Status de resultado da ação
 */
export type AuditStatus = 'success' | 'failure' | 'warning'

/**
 * Interface de um log de auditoria
 */
export type AuditLog = AuditLogDto
export type AuditStats = AuditStatsDto

// ==================== Storages ====================

export type StorageProvider =
  | 'aws_s3'
  | 'minio'
  | 'cloudflare_r2'
  | 'google_gcs'
  | 'azure_blob'
  | 'sftp'
  | 'local'

export type Storage = Omit<StorageDto, 'provider' | 'type' | 'status'> & {
  provider: StorageProvider
  type: StorageDestinationType
  status: StorageDestinationStatus
  config?: Record<string, unknown> | null
}
export type StorageDetail = StorageDetailDto

export interface BucketObjectReplica {
  locationType: 'local' | 'remote'
  storageId: number | null
  storageName: string
  provider: StorageProvider
  path: string
}

export interface BucketObject {
  key: string
  name: string
  size: number | null
  lastModified: string | null
  isDirectory: boolean
  etag?: string
  replicas?: BucketObjectReplica[]
}

export type CopyJobStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface CopyJob {
  id: string
  sourceStorageId: number
  destinationStorageId: number
  status: CopyJobStatus
  filesTransferred: number
  totalFiles: number | null
  bytesTransferred: number
  error?: string
  startedAt: string
  completedAt?: string
}

export type ArchiveJobStatus = 'pending' | 'building' | 'ready' | 'expired' | 'failed'

export interface ArchiveJob {
  id: string
  storageId: number
  path: string | null
  status: ArchiveJobStatus
  totalFiles: number | null
  processedFiles: number
  downloadUrl?: string
  expiresAt?: string
  error?: string
}

export interface CreateStoragePayload {
  name: string
  type: StorageDestinationType
  provider: StorageProvider
  status?: StorageDestinationStatus
  isDefault?: boolean
  config?: StorageDestinationConfigPayload
}

export interface UpdateStoragePayload {
  name?: string
  type?: StorageDestinationType
  provider?: StorageProvider
  status?: StorageDestinationStatus
  isDefault?: boolean
  config?: StorageDestinationConfigPayload
}

export interface CopyStoragePayload {
  destinationId: number
  sourcePath?: string
  destinationPath?: string
  dryRun?: boolean
  deleteExtraneous?: boolean
}

export interface BrowseResult {
  objects: BucketObject[]
  cursor: string | null
  path: string
}

export interface DeleteStorageObjectPayload {
  key: string
  isDirectory: boolean
}

// ==================== Docker Manager ====================

export interface DockerContainerPort {
  IP?: string
  PrivatePort: number
  PublicPort?: number
  Type: string
}

export type DockerContainerState =
  | 'running'
  | 'stopped'
  | 'paused'
  | 'restarting'
  | 'dead'
  | 'created'
  | 'exited'
  | string

export interface DockerContainerSummary {
  id: string
  names: string[]
  image: string
  imageId: string
  state: DockerContainerState
  status: string
  labels: Record<string, string>
  ports: DockerContainerPort[]
  created: number
}

export interface DockerContainerGroup {
  projectName: string
  containers: DockerContainerSummary[]
}

export interface DockerMount {
  type: string
  name?: string
  source: string
  destination: string
  mode: string
  rw: boolean
}

export interface DockerNetworkEndpoint {
  networkId: string
  networkName: string
  ipAddress: string
  gateway: string
  aliases: string[] | null
}

export interface DockerContainerDetail {
  id: string
  name: string
  image: string
  imageId: string
  created: string
  state: {
    status: string
    running: boolean
    paused: boolean
    restarting: boolean
    pid: number
    startedAt: string
    finishedAt: string
    exitCode: number
  }
  config: {
    hostname: string
    env: string[]
    cmd: string[] | null
    entrypoint: string[] | null
    labels: Record<string, string>
    workingDir: string
    user: string
  }
  hostConfig: {
    restartPolicy: {
      name: string
      maximumRetryCount: number
    }
    networkMode: string
  }
  mounts: DockerMount[]
  networks: DockerNetworkEndpoint[]
}

export interface DockerVolumeSummary {
  name: string
  driver: string
  mountpoint: string
  labels: Record<string, string>
  scope: string
  createdAt?: string
}

export interface DockerVolumeDetail extends DockerVolumeSummary {
  options: Record<string, string>
  status?: Record<string, unknown>
}

export interface DockerNetworkContainer {
  containerId: string
  name: string
  macAddress: string
  ipv4Address: string
  ipv6Address: string
}

export interface DockerNetworkSummary {
  id: string
  name: string
  driver: string
  scope: string
  ipam: {
    driver: string
    config: Array<{ subnet?: string; gateway?: string }>
  }
  internal: boolean
  connectedContainers: number
  labels: Record<string, string>
  created: string
}

export interface DockerNetworkDetail extends DockerNetworkSummary {
  containers: Record<string, DockerNetworkContainer>
  options: Record<string, string>
}

export type DockerDiagnosticTool = 'ping' | 'port_scan' | 'curl'

export type DockerDiagnosticStatus = 'pending' | 'running' | 'completed' | 'failed'

export interface DockerDiagnosticJob {
  id: string
  tool: DockerDiagnosticTool
  status: DockerDiagnosticStatus
  target: string
  port: number | null
  count: number | null
  timeoutMs: number | null
  startedAt: string
  completedAt: string | null
  outputLines: string[]
  summary: string | null
  error: string | null
  portOpen: boolean | null
  latencyMs: number | null
}

export interface DockerDiagnosticStartPayload {
  tool: DockerDiagnosticTool
  target: string
  port?: number
  count?: number
  timeoutMs?: number
}

export interface DockerDiagnosticTargetOption {
  label: string
  value: string
}

export interface DockerDiagnosticPreset {
  tool?: DockerDiagnosticTool
  target?: string
  port?: number | null
  count?: number | null
  timeoutMs?: number | null
  contextLabel?: string
  suggestedTargets?: DockerDiagnosticTargetOption[]
}

export interface DockerImageSummary {
  id: string
  parentId: string
  repoTags: string[]
  repoDigests: string[]
  created: number
  size: number
  sharedSize: number
  labels: Record<string, string>
  containers: number
}

export interface DockerImageDetail {
  id: string
  repoTags: string[]
  created: string
  size: number
  config: {
    env: string[] | null
    cmd: string[] | null
    entrypoint: string[] | null
    labels: Record<string, string>
    workingDir: string
    user: string
  }
  rootFs: {
    type: string
    layers: string[]
  }
}

export interface DockerLogEntry {
  timestamp: string
  stream: 'stdout' | 'stderr'
  message: string
}

export interface DockerActionResult {
  success: boolean
  message: string
}

export interface DockerPruneResult {
  imagesDeleted: Array<{ untagged?: string; deleted?: string }>
  spaceReclaimed: number
}

export interface DockerLogsParams {
  tail?: number | 'all'
  since?: number
  until?: number
  timestamps?: boolean
}

