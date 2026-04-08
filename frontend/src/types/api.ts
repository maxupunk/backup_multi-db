/**
 * Tipos base para a API
 */

// Status de uma conexão
export type ConnectionStatus = 'active' | 'inactive' | 'error'

// Tipos de banco de dados suportados
export type DatabaseType = 'mysql' | 'mariadb' | 'postgresql'

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
export interface ConnectionDatabase {
  id: number
  databaseName: string
  enabled: boolean
}

/**
 * Interface de uma conexão de banco de dados
 */
export interface Connection {
  id: number
  name: string
  type: DatabaseType
  host: string
  port: number
  username: string
  storageDestinationId?: number | null
  scheduleFrequency: ScheduleFrequency | null
  scheduleEnabled: boolean
  status: ConnectionStatus
  lastError: string | null
  lastTestedAt: string | null
  lastBackupAt: string | null
  options: {
    ssl?: boolean
    charset?: string
  } | null
  createdAt: string
  updatedAt: string
  databases?: ConnectionDatabase[]
  backups?: BackupSummary[]
}

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

export type DockerHostResolutionSource = 'docker_dns' | 'host_ip' | 'fallback'

export interface DockerPortOption {
  containerPort: number
  hostPort: number
  protocol: string
  display: string
}

export interface DockerHostSuggestion {
  containerId: string
  containerName: string
  databaseTypeHint: DatabaseType | null
  sameNetwork: boolean
  suggestedHost: string
  hostResolutionSource: DockerHostResolutionSource
  networkNames: string[]
  portOptions: DockerPortOption[]
  hasExternalPort: boolean
  connectivityWarning: string | null
}

export interface DockerHostsResponseData {
  dockerAvailable: boolean
  unavailableReason: string | null
  backendContainerId: string | null
  hosts: DockerHostSuggestion[]
}

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

export interface StorageDestination {
  id: number
  name: string
  type: StorageDestinationType
  status: StorageDestinationStatus
  isDefault: boolean
  createdAt: string
  updatedAt: string
  config?: Record<string, unknown> | null
}

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
export interface Backup {
  id: number
  connectionId: number
  connectionDatabaseId: number | null
  databaseName: string
  status: BackupStatus
  filePath: string | null
  fileName: string | null
  fileSize: number | null
  checksum: string | null
  compressed: boolean
  retentionType: RetentionType
  protected: boolean
  startedAt: string | null
  finishedAt: string | null
  durationSeconds: number | null
  errorMessage: string | null
  exitCode: number | null
  trigger: BackupTrigger
  metadata: { isRestoreSafetyBackup?: boolean; [key: string]: unknown } | null
  createdAt: string
  updatedAt: string
  connection?: ConnectionSummary
}

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
export interface BackupSummary {
  id: number
  status: BackupStatus
  databaseName?: string
  fileName?: string
  fileSize: number | null
  retentionType?: RetentionType
  trigger?: BackupTrigger
  createdAt: string
  finishedAt?: string | null
  durationSeconds?: number | null
}

/**
 * Resumo de uma conexão (para listagens)
 */
export interface ConnectionSummary {
  id: number
  name: string
  type: DatabaseType
  host: string
}

/**
 * Resposta de sucesso da API
 */
export interface ApiResponse<T = unknown> {
  success: boolean
  message?: string
  data?: T
  error?: string
}

/**
 * Resposta paginada
 */
export interface PaginatedResponse<T> {
  success: boolean
  data: {
    meta: {
      total: number
      perPage: number
      currentPage: number
      lastPage: number
      firstPage: number
    }
    data: T[]
  }
}

/**
 * Informações de espaço de armazenamento
 */
export interface StorageSpaceInfo {
  destinationId: number | null
  destinationName: string
  type: string
  spaceAvailable: boolean
  totalBytes: number
  usedBytes: number
  freeBytes: number
  usedPercent: number
  freePercent: number
  isLowSpace: boolean
  lowSpaceThreshold: number
}

export interface JobsSystemStatus {
  isRunning: boolean
  activeJobs: number
  status: 'ok' | 'down'
}

export interface CpuResourceMetrics {
  usagePercent: number
  cores: number
  model: string
}

export interface MemoryResourceMetrics {
  totalBytes: number
  usedBytes: number
  freeBytes: number
  usagePercent: number
}

export interface SystemResourceMetrics {
  cpu: CpuResourceMetrics
  memory: MemoryResourceMetrics
}

export interface SystemStatus {
  version: string
  hostname: string
  platform: string
  architecture: string
  nodeVersion: string
  uptimeSeconds: number
  resources: SystemResourceMetrics
  jobs: JobsSystemStatus
}

export interface DockerContainerResourceMetrics {
  containerId: string
  containerName: string
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
export interface ConnectionTestResult {
  latencyMs: number
  version: string
}

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
export interface ImportBackupResult {
  backup: Backup
  format: ImportedFileFormat
  checksum: string
  fileSize: number
  integrity: IntegrityCheckResult | null
}

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

/**
 * Tipos de entidades auditáveis
 */
export type AuditEntityType = 'connection' | 'backup' | 'settings'

/**
 * Status de resultado da ação
 */
export type AuditStatus = 'success' | 'failure' | 'warning'

/**
 * Interface de um log de auditoria
 */
export interface AuditLog {
  id: number
  action: AuditAction
  actionDescription: string
  actionIcon: string
  entityType: AuditEntityType
  entityId: number | null
  entityName: string | null
  description: string
  details: Record<string, unknown> | null
  status: AuditStatus
  statusColor: string
  errorMessage: string | null
  ipAddress: string | null
  userAgent?: string | null
  createdAt: string
}

/**
 * Estatísticas de auditoria
 */
export interface AuditStats {
  total: number
  today: number
  lastWeek: number
  byStatus: {
    success: number
    failure: number
  }
  byAction: {
    action: AuditAction
    description: string
    count: number
  }[]
}

// ==================== Storages ====================

export type StorageProvider =
  | 'aws_s3'
  | 'minio'
  | 'cloudflare_r2'
  | 'google_gcs'
  | 'azure_blob'
  | 'sftp'
  | 'local'

export interface Storage extends StorageDestination {
  provider: StorageProvider
}

export interface BucketObject {
  key: string
  name: string
  size: number | null
  lastModified: string | null
  isDirectory: boolean
  etag?: string
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
