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

// Tipo de retenção
export type RetentionType = 'hourly' | 'daily' | 'weekly' | 'monthly' | 'yearly'

// Trigger do backup
export type BackupTrigger = 'scheduled' | 'manual'

/**
 * Interface de uma conexão de banco de dados
 */
export interface Connection {
  id: number
  name: string
  type: DatabaseType
  host: string
  port: number
  database: string
  username: string
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
  database: string
  username: string
  password: string
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
  database?: string
  username?: string
  password?: string
  scheduleFrequency?: ScheduleFrequency | null
  scheduleEnabled?: boolean
  options?: {
    ssl?: boolean
    charset?: string
  } | null
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
}

/**
 * Interface de um backup
 */
export interface Backup {
  id: number
  connectionId: number
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
  createdAt: string
  updatedAt: string
  connection?: ConnectionSummary
}

/**
 * Resumo de um backup (para listagens)
 */
export interface BackupSummary {
  id: number
  status: BackupStatus
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
  database: string
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
 * Estatísticas do dashboard
 */
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

// ==================== Audit Logs ====================

/**
 * Ações auditáveis no sistema
 */
export type AuditAction =
  | 'connection.created'
  | 'connection.updated'
  | 'connection.deleted'
  | 'connection.tested'
  | 'backup.started'
  | 'backup.completed'
  | 'backup.failed'
  | 'backup.deleted'
  | 'backup.downloaded'
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
