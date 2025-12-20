import { HttpContext } from '@adonisjs/core/http'
import AuditLog from '#models/audit_log'
import type { AuditAction, AuditEntityType, AuditStatus, AuditDetails } from '#models/audit_log'

/**
 * Dados para criação de um log de auditoria
 */
export interface AuditLogData {
  action: AuditAction
  entityType: AuditEntityType
  entityId?: number | null
  entityName?: string | null
  description: string
  details?: AuditDetails | null
  status?: AuditStatus
  errorMessage?: string | null
}

/**
 * Service responsável por registrar logs de auditoria.
 * Utilizando para rastrear todas as ações importantes do sistema.
 */
export class AuditService {
  /**
   * Registra um log de auditoria
   */
  static async log(data: AuditLogData, ctx?: HttpContext): Promise<AuditLog> {
    const logEntry = await AuditLog.create({
      action: data.action,
      entityType: data.entityType,
      entityId: data.entityId ?? null,
      entityName: data.entityName ?? null,
      description: data.description,
      details: data.details ?? null,
      status: data.status ?? 'success',
      errorMessage: data.errorMessage ?? null,
      ipAddress: ctx?.request.ip() ?? null,
      userAgent: ctx?.request.header('user-agent')?.substring(0, 500) ?? null,
    })

    return logEntry
  }

  // ==================== Métodos de Conexão ====================

  /**
   * Registra criação de conexão
   */
  static async logConnectionCreated(
    connectionId: number,
    connectionName: string,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'connection.created',
        entityType: 'connection',
        entityId: connectionId,
        entityName: connectionName,
        description: `Conexão "${connectionName}" foi criada`,
      },
      ctx
    )
  }

  /**
   * Registra atualização de conexão
   */
  static async logConnectionUpdated(
    connectionId: number,
    connectionName: string,
    changes?: Record<string, { from: unknown; to: unknown }>,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'connection.updated',
        entityType: 'connection',
        entityId: connectionId,
        entityName: connectionName,
        description: `Conexão "${connectionName}" foi atualizada`,
        details: changes ? { changes } : null,
      },
      ctx
    )
  }

  /**
   * Registra exclusão de conexão
   */
  static async logConnectionDeleted(
    connectionId: number,
    connectionName: string,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'connection.deleted',
        entityType: 'connection',
        entityId: connectionId,
        entityName: connectionName,
        description: `Conexão "${connectionName}" foi removida`,
      },
      ctx
    )
  }

  /**
   * Registra teste de conexão
   */
  static async logConnectionTested(
    connectionId: number,
    connectionName: string,
    success: boolean,
    errorMessage?: string,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'connection.tested',
        entityType: 'connection',
        entityId: connectionId,
        entityName: connectionName,
        description: success
          ? `Teste de conexão "${connectionName}" bem-sucedido`
          : `Teste de conexão "${connectionName}" falhou`,
        status: success ? 'success' : 'failure',
        errorMessage: errorMessage ?? null,
      },
      ctx
    )
  }

  // ==================== Métodos de Backup ====================

  /**
   * Registra início de backup
   */
  static async logBackupStarted(
    backupId: number,
    connectionName: string,
    trigger: 'manual' | 'scheduled',
    ctx?: HttpContext
  ): Promise<AuditLog> {
    const triggerLabel = trigger === 'manual' ? 'manualmente' : 'por agendamento'
    return this.log(
      {
        action: 'backup.started',
        entityType: 'backup',
        entityId: backupId,
        entityName: connectionName,
        description: `Backup de "${connectionName}" iniciado ${triggerLabel}`,
        details: { trigger },
      },
      ctx
    )
  }

  /**
   * Registra backup concluído
   */
  static async logBackupCompleted(
    backupId: number,
    connectionName: string,
    fileSize: number,
    durationSeconds: number,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'backup.completed',
        entityType: 'backup',
        entityId: backupId,
        entityName: connectionName,
        description: `Backup de "${connectionName}" concluído com sucesso`,
        details: { fileSize, durationSeconds },
      },
      ctx
    )
  }

  /**
   * Registra backup falho
   */
  static async logBackupFailed(
    backupId: number,
    connectionName: string,
    errorMessage: string,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'backup.failed',
        entityType: 'backup',
        entityId: backupId,
        entityName: connectionName,
        description: `Backup de "${connectionName}" falhou`,
        status: 'failure',
        errorMessage,
      },
      ctx
    )
  }

  /**
   * Registra exclusão de backup
   */
  static async logBackupDeleted(
    backupId: number,
    connectionName: string,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'backup.deleted',
        entityType: 'backup',
        entityId: backupId,
        entityName: connectionName,
        description: `Backup de "${connectionName}" foi removido`,
      },
      ctx
    )
  }

  /**
   * Registra download de backup
   */
  static async logBackupDownloaded(
    backupId: number,
    connectionName: string,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'backup.downloaded',
        entityType: 'backup',
        entityId: backupId,
        entityName: connectionName,
        description: `Backup de "${connectionName}" foi baixado`,
      },
      ctx
    )
  }

  // ==================== Métodos de Configurações ====================

  /**
   * Registra atualização de configurações
   */
  static async logSettingsUpdated(
    changes?: Record<string, { from: unknown; to: unknown }>,
    ctx?: HttpContext
  ): Promise<AuditLog> {
    return this.log(
      {
        action: 'settings.updated',
        entityType: 'settings',
        description: 'Configurações do sistema foram atualizadas',
        details: changes ? { changes } : null,
      },
      ctx
    )
  }
}
