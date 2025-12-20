import { DateTime } from 'luxon'
import { BaseModel, column } from '@adonisjs/lucid/orm'

/**
 * Ações auditáveis disponíveis no sistema
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
 * Tipos de entidades que podem ser auditadas
 */
export type AuditEntityType = 'connection' | 'backup' | 'settings'

/**
 * Status do resultado da ação
 */
export type AuditStatus = 'success' | 'failure' | 'warning'

/**
 * Detalhes adicionais da auditoria
 */
export interface AuditDetails {
  changes?: Record<string, { from: unknown; to: unknown }>
  metadata?: Record<string, unknown>
  [key: string]: unknown
}

/**
 * Model para logs de auditoria.
 * Registra todas as ações importantes realizadas no sistema.
 */
export default class AuditLog extends BaseModel {
  @column({ isPrimary: true })
  declare id: number

  @column()
  declare action: AuditAction

  @column()
  declare entityType: AuditEntityType

  @column()
  declare entityId: number | null

  @column()
  declare entityName: string | null

  @column()
  declare description: string

  @column({
    prepare: (value: AuditDetails) => (value ? JSON.stringify(value) : null),
    consume: (value: string) => (value ? JSON.parse(value) : null),
  })
  declare details: AuditDetails | null

  @column()
  declare ipAddress: string | null

  @column()
  declare userAgent: string | null

  @column()
  declare status: AuditStatus

  @column()
  declare errorMessage: string | null

  @column.dateTime({ autoCreate: true })
  declare createdAt: DateTime

  // ==================== Métodos Estáticos ====================

  /**
   * Obtém a descrição legível de uma ação
   */
  static getActionDescription(action: AuditAction): string {
    const descriptions: Record<AuditAction, string> = {
      'connection.created': 'Conexão criada',
      'connection.updated': 'Conexão atualizada',
      'connection.deleted': 'Conexão removida',
      'connection.tested': 'Conexão testada',
      'backup.started': 'Backup iniciado',
      'backup.completed': 'Backup concluído',
      'backup.failed': 'Backup falhou',
      'backup.deleted': 'Backup removido',
      'backup.downloaded': 'Backup baixado',
      'settings.updated': 'Configurações atualizadas',
    }
    return descriptions[action]
  }

  /**
   * Obtém o ícone para uma ação (para uso no frontend)
   */
  static getActionIcon(action: AuditAction): string {
    const icons: Record<AuditAction, string> = {
      'connection.created': 'mdi-database-plus',
      'connection.updated': 'mdi-database-edit',
      'connection.deleted': 'mdi-database-remove',
      'connection.tested': 'mdi-database-check',
      'backup.started': 'mdi-play-circle',
      'backup.completed': 'mdi-check-circle',
      'backup.failed': 'mdi-alert-circle',
      'backup.deleted': 'mdi-delete',
      'backup.downloaded': 'mdi-download',
      'settings.updated': 'mdi-cog',
    }
    return icons[action]
  }

  /**
   * Obtém a cor para um status (para uso no frontend)
   */
  static getStatusColor(status: AuditStatus): string {
    const colors: Record<AuditStatus, string> = {
      success: 'success',
      failure: 'error',
      warning: 'warning',
    }
    return colors[status]
  }

  // ==================== Métodos de Instância ====================

  /**
   * Retorna uma representação resumida do log
   */
  getSummary(): string {
    const statusEmoji = this.status === 'success' ? '✓' : this.status === 'failure' ? '✗' : '⚠'
    return `[${statusEmoji}] ${this.description}`
  }

  /**
   * Verifica se a ação foi bem-sucedida
   */
  isSuccessful(): boolean {
    return this.status === 'success'
  }
}
