import transmit from '@adonisjs/transmit/services/main'
import logger from '@adonisjs/core/services/logger'
import { NOTIFICATION_CHANNELS } from '#services/notification_service'

/**
 * Estágios possíveis de um backup
 */
export type BackupStageKey =
  | 'starting'
  | 'dumping'
  | 'compressing'
  | 'uploading'
  | 'completed'
  | 'failed'

/**
 * Emissor de progresso de backup via SSE (Server-Sent Events).
 *
 * Responsabilidade única: emitir eventos de progresso durante o processo de backup.
 * Os bytes escritos no gzip são contados para calcular o progresso (quando estimativa está disponível).
 *
 * Eventos de progresso → canal `notifications/backup-progress` (para UI de progresso)
 * Notificações de marco → canal `notifications/backup` (para toasts — via NotificationService existente)
 */
export class BackupProgressEmitter {
  private readonly operationId: string
  private readonly connectionName: string
  private readonly databaseName: string
  private lastEmitTime: number = 0
  private static readonly THROTTLE_MS = 500

  constructor(connectionName: string, databaseName: string) {
    this.operationId = `backup-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
    this.connectionName = connectionName
    this.databaseName = databaseName
  }

  getOperationId(): string {
    return this.operationId
  }

  /**
   * Estágio: backup iniciado
   */
  started(): void {
    this.emitProgress('starting', 0, 'Iniciando backup...')
  }

  /**
   * Estágio: executando dump do banco
   */
  dumping(): void {
    this.emitProgress('dumping', 0, 'Executando dump do banco de dados...')
  }

  /**
   * Atualiza o progresso do dump/compressão.
   * Como não temos tamanho total a priori, reportamos bytes processados.
   */
  progress(bytesWritten: number): void {
    const now = Date.now()

    if (now - this.lastEmitTime < BackupProgressEmitter.THROTTLE_MS) {
      return
    }

    this.lastEmitTime = now
    const formattedSize = this.formatBytes(bytesWritten)
    this.emitProgress(
      'compressing',
      0,
      `Comprimindo dados... ${formattedSize} escritos`,
      bytesWritten
    )
  }

  /**
   * Estágio: fazendo upload para destino remoto
   */
  uploading(): void {
    this.emitProgress('uploading', 0, 'Enviando para armazenamento remoto...')
  }

  /**
   * Backup concluído com sucesso
   */
  completed(fileSize: number, durationSeconds: number): void {
    const formattedSize = this.formatBytes(fileSize)
    this.emitProgress(
      'completed',
      100,
      `Backup concluído em ${durationSeconds}s (${formattedSize})`
    )
  }

  /**
   * Backup falhou
   */
  failed(error: string): void {
    this.emitProgress('failed', 0, error)
  }

  /**
   * Emite evento de progresso para o canal dedicado de backup-progress
   */
  private emitProgress(
    stage: BackupStageKey,
    progress: number,
    message: string,
    bytesWritten?: number
  ): void {
    const event: Record<string, string | number> = {
      operationId: this.operationId,
      type: 'backup',
      connectionName: this.connectionName,
      databaseName: this.databaseName,
      stage,
      progress,
      message,
      timestamp: new Date().toISOString(),
    }

    if (bytesWritten !== undefined) {
      event.bytesWritten = bytesWritten
    }

    try {
      transmit.broadcast(NOTIFICATION_CHANNELS.BACKUP_PROGRESS, event)
    } catch (err) {
      logger.error(`[BackupProgress] Erro ao emitir progresso: ${err}`)
    }
  }

  private formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return `${Number.parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`
  }
}
