import type { HttpContext } from '@adonisjs/core/http'
import AuditLog from '#models/audit_log'
import type { AuditAction, AuditEntityType, AuditStatus } from '#models/audit_log'

/**
 * Controller para gerenciamento de logs de auditoria.
 * Fornece endpoints para listagem e visualização de logs.
 */
export default class AuditLogsController {
  /**
   * Lista todos os logs de auditoria com filtros e paginação
   * GET /api/audit-logs
   */
  async index({ request, response }: HttpContext) {
    try {
      const page = request.input('page', 1)
      const limit = Math.min(request.input('limit', 50), 100) // Máximo 100 por página

      // Filtros opcionais
      const action = request.input('action') as AuditAction | undefined
      const entityType = request.input('entityType') as AuditEntityType | undefined
      const entityId = request.input('entityId') as number | undefined
      const status = request.input('status') as AuditStatus | undefined
      const startDate = request.input('startDate') as string | undefined
      const endDate = request.input('endDate') as string | undefined

      let query = AuditLog.query().orderBy('createdAt', 'desc')

      // Aplicar filtros
      if (action) {
        query = query.where('action', action)
      }

      if (entityType) {
        query = query.where('entityType', entityType)
      }

      if (entityId) {
        query = query.where('entityId', entityId)
      }

      if (status) {
        query = query.where('status', status)
      }

      if (startDate) {
        query = query.where('createdAt', '>=', startDate)
      }

      if (endDate) {
        query = query.where('createdAt', '<=', endDate)
      }

      const logs = await query.paginate(page, limit)

      return response.ok({
        success: true,
        data: logs.all().map((log) => ({
          id: log.id,
          action: log.action,
          actionDescription: AuditLog.getActionDescription(log.action),
          actionIcon: AuditLog.getActionIcon(log.action),
          entityType: log.entityType,
          entityId: log.entityId,
          entityName: log.entityName,
          description: log.description,
          details: log.details,
          status: log.status,
          statusColor: AuditLog.getStatusColor(log.status),
          errorMessage: log.errorMessage,
          ipAddress: log.ipAddress,
          createdAt: log.createdAt,
        })),
        meta: logs.getMeta(),
      })
    } catch (error) {
      return response.internalServerError({
        success: false,
        message: 'Erro ao listar logs de auditoria',
        error: error instanceof Error ? error.message : 'Erro desconhecido',
      })
    }
  }

  /**
   * Obtém um log de auditoria específico
   * GET /api/audit-logs/:id
   */
  async show({ params, response }: HttpContext) {
    try {
      const log = await AuditLog.find(params.id)

      if (!log) {
        return response.notFound({
          success: false,
          message: 'Log de auditoria não encontrado',
        })
      }

      return response.ok({
        success: true,
        data: {
          id: log.id,
          action: log.action,
          actionDescription: AuditLog.getActionDescription(log.action),
          actionIcon: AuditLog.getActionIcon(log.action),
          entityType: log.entityType,
          entityId: log.entityId,
          entityName: log.entityName,
          description: log.description,
          details: log.details,
          status: log.status,
          statusColor: AuditLog.getStatusColor(log.status),
          errorMessage: log.errorMessage,
          ipAddress: log.ipAddress,
          userAgent: log.userAgent,
          createdAt: log.createdAt,
        },
      })
    } catch (error) {
      return response.internalServerError({
        success: false,
        message: 'Erro ao obter log de auditoria',
        error: error instanceof Error ? error.message : 'Erro desconhecido',
      })
    }
  }

  /**
   * Obtém estatísticas dos logs de auditoria
   * GET /api/audit-logs/stats
   */
  async stats({ response }: HttpContext) {
    try {
      const { DateTime } = await import('luxon')
      const today = DateTime.now().startOf('day')
      const lastWeek = DateTime.now().minus({ days: 7 }).startOf('day')

      const [totalLogs, logsToday, logsLastWeek, failedLogs, successLogs, actionCounts] =
        await Promise.all([
          AuditLog.query().count('* as total').first(),
          AuditLog.query().where('createdAt', '>=', today.toSQL()!).count('* as total').first(),
          AuditLog.query().where('createdAt', '>=', lastWeek.toSQL()!).count('* as total').first(),
          AuditLog.query().where('status', 'failure').count('* as total').first(),
          AuditLog.query().where('status', 'success').count('* as total').first(),
          AuditLog.query()
            .select('action')
            .count('* as count')
            .groupBy('action')
            .orderBy('count', 'desc'),
        ])

      return response.ok({
        success: true,
        data: {
          total: Number(totalLogs?.$extras.total ?? 0),
          today: Number(logsToday?.$extras.total ?? 0),
          lastWeek: Number(logsLastWeek?.$extras.total ?? 0),
          byStatus: {
            success: Number(successLogs?.$extras.total ?? 0),
            failure: Number(failedLogs?.$extras.total ?? 0),
          },
          byAction: actionCounts.map((item) => ({
            action: item.action,
            description: AuditLog.getActionDescription(item.action),
            count: Number(item.$extras.count),
          })),
        },
      })
    } catch (error) {
      return response.internalServerError({
        success: false,
        message: 'Erro ao obter estatísticas de auditoria',
        error: error instanceof Error ? error.message : 'Erro desconhecido',
      })
    }
  }
}
