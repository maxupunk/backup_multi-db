import transmit from '@adonisjs/transmit/services/main'
import logger from '@adonisjs/core/services/logger'

/**
 * Tipo compatível com Broadcastable do Transmit
 */
type BroadcastableValue = { [key: string]: BroadcastableValue } | string | number | boolean | null | BroadcastableValue[]

/**
 * Tipos de notificação suportados
 */
export type NotificationType =
  | 'info'
  | 'success'
  | 'warning'
  | 'error'

/**
 * Categorias de notificação para filtro no frontend
 */
export type NotificationCategory =
  | 'system'
  | 'backup'
  | 'storage'
  | 'connection'
  | 'auth'

/**
 * Estrutura de uma notificação
 */
export interface Notification {
  id: string
  type: NotificationType
  category: NotificationCategory
  title: string
  message: string
  timestamp: string
  data?: Record<string, BroadcastableValue>
}

/**
 * Canais de transmissão disponíveis
 */
export const NOTIFICATION_CHANNELS = {
  /** Canal global para todas as notificações */
  GLOBAL: 'notifications/global',
  /** Canal sistema (backend iniciado, shutdown, etc.) */
  SYSTEM: 'notifications/system',
  /** Canal de backups (iniciado, concluído, falhou) */
  BACKUP: 'notifications/backup',
  /** Canal de armazenamento (espaço baixo, erros) */
  STORAGE: 'notifications/storage',
  /** Canal de conexões (teste, status) */
  CONNECTION: 'notifications/connection',
} as const

/**
 * Serviço unificado de notificações via Server-Sent Events (SSE)
 *
 * Utiliza o @adonisjs/transmit para enviar notificações em tempo real
 * para os clientes conectados.
 */
export class NotificationService {
  /**
   * Gera um ID único para a notificação
   */
  private static generateId(): string {
    return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
  }

  /**
   * Cria uma notificação base
   */
  private static createNotification(
    type: NotificationType,
    category: NotificationCategory,
    title: string,
    message: string,
    data?: Record<string, BroadcastableValue>
  ): Notification {
    return {
      id: this.generateId(),
      type,
      category,
      title,
      message,
      timestamp: new Date().toISOString(),
      data,
    }
  }

  /**
   * Envia uma notificação para um canal específico e também para o canal global
   */
  private static broadcast(
    channel: string,
    notification: Notification
  ): void {
    try {
      // Cria um payload limpo sem valores undefined
      // O Transmit Broadcastable não aceita undefined
      const payload: { [key: string]: BroadcastableValue } = {
        id: notification.id,
        type: notification.type,
        category: notification.category,
        title: notification.title,
        message: notification.message,
        timestamp: notification.timestamp,
      }

      // Adiciona data apenas se existir
      if (notification.data) {
        payload.data = notification.data
      }

      // Envia para o canal específico
      transmit.broadcast(channel, payload)

      // Também envia para o canal global (para UI que mostra todas as notificações)
      if (channel !== NOTIFICATION_CHANNELS.GLOBAL) {
        transmit.broadcast(NOTIFICATION_CHANNELS.GLOBAL, payload)
      }

      logger.debug(`[Notification] Broadcasting to ${channel}: ${notification.title}`)
    } catch (error) {
      logger.error(`[Notification] Erro ao enviar notificação: ${error}`)
    }
  }

  // ==================== Notificações de Sistema ====================

  /**
   * Notifica que o backend foi iniciado
   */
  static systemStarted(): void {
    const notification = this.createNotification(
      'success',
      'system',
      'Sistema Iniciado',
      'O servidor backend foi iniciado com sucesso.',
      { event: 'system.started' }
    )
    this.broadcast(NOTIFICATION_CHANNELS.SYSTEM, notification)
    logger.info('[System] Backend iniciado - notificação enviada')
  }

  /**
   * Notifica que o backend está sendo encerrado
   */
  static systemShutdown(): void {
    const notification = this.createNotification(
      'warning',
      'system',
      'Sistema Encerrando',
      'O servidor backend está sendo encerrado.',
      { event: 'system.shutdown' }
    )
    this.broadcast(NOTIFICATION_CHANNELS.SYSTEM, notification)
    logger.info('[System] Backend encerrando - notificação enviada')
  }

  /**
   * Envia uma notificação genérica do sistema
   */
  static systemInfo(title: string, message: string, data?: Record<string, BroadcastableValue>): void {
    const notification = this.createNotification('info', 'system', title, message, data)
    this.broadcast(NOTIFICATION_CHANNELS.SYSTEM, notification)
  }

  /**
   * Envia um erro do sistema
   */
  static systemError(title: string, message: string, data?: Record<string, BroadcastableValue>): void {
    const notification = this.createNotification('error', 'system', title, message, data)
    this.broadcast(NOTIFICATION_CHANNELS.SYSTEM, notification)
  }

  // ==================== Notificações de Backup ====================

  /**
   * Notifica que um backup foi iniciado
   */
  static backupStarted(
    connectionName: string,
    connectionId: number,
    trigger: string
  ): void {
    const notification = this.createNotification(
      'info',
      'backup',
      'Backup Iniciado',
      `O backup de "${connectionName}" foi iniciado (${trigger === 'manual' ? 'manual' : 'agendado'}).`,
      {
        event: 'backup.started',
        connectionId,
        connectionName,
        trigger,
      }
    )
    this.broadcast(NOTIFICATION_CHANNELS.BACKUP, notification)
  }

  /**
   * Notifica que um backup foi concluído com sucesso
   */
  static backupCompleted(
    connectionName: string,
    connectionId: number,
    backupId: number,
    fileName: string,
    fileSize: number
  ): void {
    const notification = this.createNotification(
      'success',
      'backup',
      'Backup Concluído',
      `O backup de "${connectionName}" foi concluído com sucesso. Arquivo: ${fileName}`,
      {
        event: 'backup.completed',
        connectionId,
        connectionName,
        backupId,
        fileName,
        fileSize,
        fileSizeFormatted: this.formatBytes(fileSize),
      }
    )
    this.broadcast(NOTIFICATION_CHANNELS.BACKUP, notification)
  }

  /**
   * Notifica que um backup falhou
   */
  static backupFailed(
    connectionName: string,
    connectionId: number,
    error: string
  ): void {
    const notification = this.createNotification(
      'error',
      'backup',
      'Backup Falhou',
      `O backup de "${connectionName}" falhou: ${error}`,
      {
        event: 'backup.failed',
        connectionId,
        connectionName,
        error,
      }
    )
    this.broadcast(NOTIFICATION_CHANNELS.BACKUP, notification)
  }

  /**
   * Notifica um alerta sobre o backup (ex: progresso lento, arquivo grande)
   */
  static backupWarning(
    connectionName: string,
    connectionId: number,
    message: string,
    data?: Record<string, BroadcastableValue>
  ): void {
    const notification = this.createNotification(
      'warning',
      'backup',
      'Alerta de Backup',
      message,
      {
        event: 'backup.warning',
        connectionId,
        connectionName,
        ...data,
      }
    )
    this.broadcast(NOTIFICATION_CHANNELS.BACKUP, notification)
  }

  // ==================== Notificações de Armazenamento ====================

  /**
   * Notifica alerta de espaço em disco baixo
   */
  static storageSpaceLow(
    destinationName: string,
    freePercent: number,
    freeBytes: number,
    destinationId?: number
  ): void {
    const data: Record<string, BroadcastableValue> = {
      event: 'storage.low_space',
      destinationName,
      freePercent,
      freeBytes,
      freeBytesFormatted: this.formatBytes(freeBytes),
    }
    if (destinationId !== undefined) {
      data.destinationId = destinationId
    }
    const notification = this.createNotification(
      'warning',
      'storage',
      'Espaço em Disco Baixo',
      `O armazenamento "${destinationName}" está com apenas ${freePercent.toFixed(1)}% livre (${this.formatBytes(freeBytes)}).`,
      data
    )
    this.broadcast(NOTIFICATION_CHANNELS.STORAGE, notification)
  }

  /**
   * Notifica erro de armazenamento
   */
  static storageError(
    destinationName: string,
    error: string,
    destinationId?: number
  ): void {
    const data: Record<string, BroadcastableValue> = {
      event: 'storage.error',
      destinationName,
      error,
    }
    if (destinationId !== undefined) {
      data.destinationId = destinationId
    }
    const notification = this.createNotification(
      'error',
      'storage',
      'Erro de Armazenamento',
      `Erro no armazenamento "${destinationName}": ${error}`,
      data
    )
    this.broadcast(NOTIFICATION_CHANNELS.STORAGE, notification)
  }

  /**
   * Notifica que o upload para destino remoto foi concluído
   */
  static storageUploadCompleted(
    destinationName: string,
    fileName: string,
    destinationId?: number
  ): void {
    const data: Record<string, BroadcastableValue> = {
      event: 'storage.upload_completed',
      destinationName,
      fileName,
    }
    if (destinationId !== undefined) {
      data.destinationId = destinationId
    }
    const notification = this.createNotification(
      'success',
      'storage',
      'Upload Concluído',
      `Arquivo "${fileName}" enviado para "${destinationName}" com sucesso.`,
      data
    )
    this.broadcast(NOTIFICATION_CHANNELS.STORAGE, notification)
  }

  // ==================== Notificações de Conexão ====================

  /**
   * Notifica que uma conexão foi testada com sucesso
   */
  static connectionTestSuccess(connectionName: string, connectionId: number): void {
    const notification = this.createNotification(
      'success',
      'connection',
      'Teste de Conexão',
      `Conexão "${connectionName}" testada com sucesso.`,
      {
        event: 'connection.test_success',
        connectionId,
        connectionName,
      }
    )
    this.broadcast(NOTIFICATION_CHANNELS.CONNECTION, notification)
  }

  /**
   * Notifica que uma conexão falhou no teste
   */
  static connectionTestFailed(
    connectionName: string,
    connectionId: number,
    error: string
  ): void {
    const notification = this.createNotification(
      'error',
      'connection',
      'Falha no Teste de Conexão',
      `Conexão "${connectionName}" falhou no teste: ${error}`,
      {
        event: 'connection.test_failed',
        connectionId,
        connectionName,
        error,
      }
    )
    this.broadcast(NOTIFICATION_CHANNELS.CONNECTION, notification)
  }

  /**
   * Notifica mudança de status da conexão
   */
  static connectionStatusChanged(
    connectionName: string,
    connectionId: number,
    newStatus: string
  ): void {
    const notification = this.createNotification(
      'info',
      'connection',
      'Status da Conexão Alterado',
      `Conexão "${connectionName}" alterada para: ${newStatus}.`,
      {
        event: 'connection.status_changed',
        connectionId,
        connectionName,
        newStatus,
      }
    )
    this.broadcast(NOTIFICATION_CHANNELS.CONNECTION, notification)
  }

  // ==================== Utilitários ====================

  /**
   * Formata bytes para exibição legível
   */
  private static formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B'

    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))

    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`
  }

  /**
   * Envia uma notificação customizada
   */
  static custom(
    type: NotificationType,
    category: NotificationCategory,
    title: string,
    message: string,
    data?: Record<string, BroadcastableValue>
  ): void {
    const notification = this.createNotification(type, category, title, message, data)
    this.broadcast(NOTIFICATION_CHANNELS.GLOBAL, notification)
  }
}
