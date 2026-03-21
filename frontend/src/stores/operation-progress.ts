/**
 * Store unificado para acompanhamento de progresso de operações (backup e restauração).
 *
 * Recebe eventos SSE dos canais notifications/restore e notifications/backup-progress
 * e mantém o estado de operações ativas para exibição na UI.
 */
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

// ==================== Tipos ====================

export type OperationType = 'backup' | 'restore'

/**
 * Estágios de restauração
 */
export type RestoreStageKey =
  | 'validating'
  | 'safety_backup'
  | 'preparing'
  | 'restoring'
  | 'completed'
  | 'failed'

/**
 * Estágios de backup
 */
export type BackupStageKey =
  | 'starting'
  | 'dumping'
  | 'compressing'
  | 'uploading'
  | 'completed'
  | 'failed'

export type StageKey = RestoreStageKey | BackupStageKey

export interface StageDefinition {
  key: StageKey
  label: string
  icon: string
}

/**
 * Estado de uma operação ativa
 */
export interface ActiveOperation {
  operationId: string
  type: OperationType
  databaseName: string
  connectionName: string
  stage: StageKey
  progress: number
  message: string
  error?: string
  bytesWritten?: number
  startedAt: string
  updatedAt: string
}

/**
 * Evento de progresso de restauração (SSE)
 */
export interface RestoreProgressEvent {
  restoreId: string
  backupId: number
  databaseName: string
  connectionName: string
  stage: RestoreStageKey
  progress: number
  message: string
  error?: string
  timestamp: string
}

/**
 * Evento de progresso de backup (SSE)
 */
export interface BackupProgressEvent {
  operationId: string
  type: 'backup'
  connectionName: string
  databaseName: string
  stage: BackupStageKey
  progress: number
  message: string
  bytesWritten?: number
  timestamp: string
}

// ==================== Definições de Estágios ====================

export const RESTORE_STAGES: StageDefinition[] = [
  { key: 'validating', label: 'Validando backup', icon: 'mdi-check-circle-outline' },
  { key: 'safety_backup', label: 'Backup de segurança', icon: 'mdi-shield-check' },
  { key: 'preparing', label: 'Preparando dados', icon: 'mdi-database-import-outline' },
  { key: 'restoring', label: 'Restaurando', icon: 'mdi-backup-restore' },
]

export const BACKUP_STAGES: StageDefinition[] = [
  { key: 'starting', label: 'Iniciando', icon: 'mdi-play-circle-outline' },
  { key: 'dumping', label: 'Dump do banco', icon: 'mdi-database-export-outline' },
  { key: 'compressing', label: 'Comprimindo', icon: 'mdi-zip-box-outline' },
  { key: 'uploading', label: 'Upload remoto', icon: 'mdi-cloud-upload-outline' },
]

// ==================== Ordem dos Estágios ====================

const RESTORE_STAGE_ORDER: Record<string, number> = {
  validating: 0,
  safety_backup: 1,
  preparing: 2,
  restoring: 3,
  completed: 4,
  failed: -1,
}

const BACKUP_STAGE_ORDER: Record<string, number> = {
  starting: 0,
  dumping: 1,
  compressing: 2,
  uploading: 3,
  completed: 4,
  failed: -1,
}

export function getStageOrder(type: OperationType): Record<string, number> {
  return type === 'restore' ? RESTORE_STAGE_ORDER : BACKUP_STAGE_ORDER
}

export function getStages(type: OperationType): StageDefinition[] {
  return type === 'restore' ? RESTORE_STAGES : BACKUP_STAGES
}

// ==================== Store ====================

export const useOperationProgressStore = defineStore('operation-progress', () => {
  const activeOperations = ref<Map<string, ActiveOperation>>(new Map())

  const operations = computed(() => Array.from(activeOperations.value.values()))

  const hasActiveOperations = computed(() =>
    operations.value.some((o) => o.stage !== 'completed' && o.stage !== 'failed')
  )

  /**
   * Processa um evento de progresso de restauração
   */
  function handleRestoreProgress(event: RestoreProgressEvent): void {
    upsertOperation({
      operationId: event.restoreId,
      type: 'restore',
      databaseName: event.databaseName,
      connectionName: event.connectionName,
      stage: event.stage,
      progress: event.progress,
      message: event.message,
      error: event.error,
      timestamp: event.timestamp,
    })
  }

  /**
   * Processa um evento de progresso de backup
   */
  function handleBackupProgress(event: BackupProgressEvent): void {
    upsertOperation({
      operationId: event.operationId,
      type: 'backup',
      databaseName: event.databaseName,
      connectionName: event.connectionName,
      stage: event.stage,
      progress: event.progress,
      message: event.message,
      bytesWritten: event.bytesWritten,
      timestamp: event.timestamp,
    })
  }

  function upsertOperation(data: {
    operationId: string
    type: OperationType
    databaseName: string
    connectionName: string
    stage: StageKey
    progress: number
    message: string
    error?: string
    bytesWritten?: number
    timestamp: string
  }): void {
    const existing = activeOperations.value.get(data.operationId)

    if (existing) {
      existing.stage = data.stage
      existing.progress = data.progress
      existing.message = data.message
      existing.error = data.error
      existing.bytesWritten = data.bytesWritten
      existing.updatedAt = data.timestamp
    } else {
      activeOperations.value.set(data.operationId, {
        operationId: data.operationId,
        type: data.type,
        databaseName: data.databaseName,
        connectionName: data.connectionName,
        stage: data.stage,
        progress: data.progress,
        message: data.message,
        error: data.error,
        bytesWritten: data.bytesWritten,
        startedAt: data.timestamp,
        updatedAt: data.timestamp,
      })
    }

    // Forçar reatividade
    activeOperations.value = new Map(activeOperations.value)

    // Auto-limpar operações finalizadas após 15 segundos
    if (data.stage === 'completed' || data.stage === 'failed') {
      setTimeout(() => {
        dismiss(data.operationId)
      }, 15000)
    }
  }

  function dismiss(operationId: string): void {
    activeOperations.value.delete(operationId)
    activeOperations.value = new Map(activeOperations.value)
  }

  function clearFinished(): void {
    for (const [id, op] of activeOperations.value) {
      if (op.stage === 'completed' || op.stage === 'failed') {
        activeOperations.value.delete(id)
      }
    }
    activeOperations.value = new Map(activeOperations.value)
  }

  return {
    operations,
    hasActiveOperations,
    handleRestoreProgress,
    handleBackupProgress,
    dismiss,
    clearFinished,
  }
})
