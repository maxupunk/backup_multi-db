import transmit from '@adonisjs/transmit/services/main'
import logger from '@adonisjs/core/services/logger'
import { NotificationService, NOTIFICATION_CHANNELS } from '#services/notification_service'

/**
 * Estágios possíveis de uma restauração
 */
export type RestoreStageKey =
  | 'validating'
  | 'safety_backup'
  | 'clearing'
  | 'preparing'
  | 'restoring'
  | 'completed'
  | 'failed'

/**
 * Evento de progresso de restauração enviado via SSE
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
 * Emissor de progresso de restauração via SSE (Server-Sent Events).
 *
 * Responsabilidade única: emitir eventos de progresso e notificações
 * de marco (iniciado, concluído, falhou) durante o processo de restauração.
 *
 * Eventos de progresso → canal `notifications/restore` (para UI de progresso)
 * Notificações de marco → canal `notifications/backup` (para toasts)
 */
export class RestoreProgressEmitter {
  private readonly restoreId: string
  private readonly backupId: number
  private readonly databaseName: string
  private readonly connectionName: string
  private lastEmittedProgress: number = -1
  private lastEmitTime: number = 0
  private static readonly THROTTLE_MS = 500

  constructor(backupId: number, databaseName: string, connectionName: string) {
    this.restoreId = `restore-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
    this.backupId = backupId
    this.databaseName = databaseName
    this.connectionName = connectionName
  }

  getRestoreId(): string {
    return this.restoreId
  }

  /**
   * Notifica que a restauração foi iniciada (marco + progresso)
   */
  started(): void {
    this.emitProgress('validating', 0, 'Restauração iniciada')
    NotificationService.restoreStarted(this.connectionName, this.backupId, this.databaseName)
  }

  /**
   * Estágio: validando backup
   */
  validating(): void {
    this.emitProgress('validating', 0, 'Validando backup...')
  }

  /**
   * Estágio: criando backup de segurança
   */
  safetyBackupStarted(): void {
    this.emitProgress('safety_backup', 0, 'Criando backup de segurança...')
  }

  /**
   * Estágio: backup de segurança concluído
   */
  safetyBackupCompleted(): void {
    this.emitProgress('safety_backup', 100, 'Backup de segurança criado com sucesso')
  }

  /**
   * Estágio: backup de segurança falhou (aborta restauração)
   */
  safetyBackupFailed(): void {
    this.emitProgress('safety_backup', 0, 'Backup de segurança falhou')
  }

  /**
   * Estágio: limpando banco de dados antes da restauração
   */
  clearingDatabase(): void {
    this.emitProgress('clearing', 0, 'Limpando banco de dados...')
  }

  /**
   * Estágio: preparando stream de dados
   */
  preparing(): void {
    this.emitProgress('preparing', 0, 'Preparando stream de dados...')
  }

  /**
   * Estágio: restaurando com percentual de progresso.
   * Atualiza com throttling para evitar flood de SSE.
   */
  restoring(progress: number): void {
    const now = Date.now()
    const roundedProgress = Math.min(100, Math.round(progress))

    if (
      roundedProgress === this.lastEmittedProgress &&
      now - this.lastEmitTime < RestoreProgressEmitter.THROTTLE_MS
    ) {
      return
    }

    this.lastEmittedProgress = roundedProgress
    this.lastEmitTime = now
    this.emitProgress(
      'restoring',
      roundedProgress,
      `Restaurando banco de dados... ${roundedProgress}%`
    )
  }

  /**
   * Notifica que a restauração foi concluída (marco + progresso)
   */
  completed(durationSeconds: number): void {
    this.emitProgress('completed', 100, `Restauração concluída em ${durationSeconds}s`)
    NotificationService.restoreCompleted(
      this.connectionName,
      this.backupId,
      this.databaseName,
      durationSeconds
    )
  }

  /**
   * Notifica que a restauração falhou (marco + progresso)
   */
  failed(error: string): void {
    this.emitProgress('failed', 0, error)
    NotificationService.restoreFailed(this.connectionName, this.backupId, this.databaseName, error)
  }

  /**
   * Emite evento de progresso para o canal dedicado de restore
   */
  private emitProgress(
    stage: RestoreStageKey,
    progress: number,
    message: string,
    error?: string
  ): void {
    const event: Record<string, string | number> = {
      restoreId: this.restoreId,
      backupId: this.backupId,
      databaseName: this.databaseName,
      connectionName: this.connectionName,
      stage,
      progress,
      message,
      timestamp: new Date().toISOString(),
    }

    if (error) {
      event.error = error
    }

    try {
      transmit.broadcast(NOTIFICATION_CHANNELS.RESTORE, event)
    } catch (err) {
      logger.error(`[RestoreProgress] Erro ao emitir progresso: ${err}`)
    }
  }
}
