import type { HttpContext } from '@adonisjs/core/http'
import { DateTime } from 'luxon'
import Backup from '#models/backup'
import Connection from '#models/connection'
import { AuditService } from '#services/audit_service'
import { BackupRetentionPolicyService } from '#services/backup_retention_policy_service'
import { DockerContainerMonitoringService } from '#services/docker_container_monitoring_service'
import { getScheduler } from '#services/scheduler_service'
import { ResourceMetricsHistoryService } from '#services/resource_metrics_history_service'
import { SystemMonitoringService } from '#services/system_monitoring_service'
import { StorageSpaceService } from '#services/storage_space_service'
import { updateBackupRetentionPolicyValidator } from '#validators/system_validator'

export default class SystemController {
  private readonly dockerContainerMonitoringService = DockerContainerMonitoringService.instance()
  private readonly backupRetentionPolicyService = new BackupRetentionPolicyService()

  async stats({ response }: HttpContext) {
    const today = DateTime.now().startOf('day')
    const [
      totalConnections,
      activeConnections,
      totalBackups,
      backupsToday,
      recentBackups,
      storageSpaces,
      system,
    ] = await Promise.all([
      Connection.query().count('* as total').first(),
      Connection.query().where('status', 'active').count('* as total').first(),
      Backup.query().count('* as total').first(),
      Backup.query().where('createdAt', '>=', today.toSQL()).count('* as total').first(),
      Backup.query().preload('connection').orderBy('createdAt', 'desc').limit(5),
      StorageSpaceService.getAllDestinationsSpaceInfo(),
      SystemMonitoringService.getOverview(),
    ])

    return response.ok({
      success: true,
      data: {
        connections: {
          total: Number(totalConnections?.$extras.total ?? 0),
          active: Number(activeConnections?.$extras.total ?? 0),
        },
        backups: {
          total: Number(totalBackups?.$extras.total ?? 0),
          today: Number(backupsToday?.$extras.total ?? 0),
        },
        recentBackups: recentBackups.map((backup) => ({
          id: backup.id,
          connectionName: backup.connection?.name ?? 'N/A',
          status: backup.status,
          fileSize: backup.fileSize,
          createdAt: backup.createdAt,
        })),
        storageSpaces,
        system,
      },
    })
  }

  async status({ response }: HttpContext) {
    return response.ok({
      success: true,
      data: await SystemMonitoringService.getOverview(),
    })
  }

  async heap({ response }: HttpContext) {
    return response.ok({
      success: true,
      data: SystemMonitoringService.getHeapSnapshot(),
    })
  }

  async containerResources({ response }: HttpContext) {
    const overview = await this.dockerContainerMonitoringService.getOverview()

    return response.ok({
      success: true,
      data: overview,
    })
  }

  async resourcesHistory({ request, response }: HttpContext) {
    const rangeHoursParam = Number(request.input('rangeHours', 24))
    const rangeHours = Number.isFinite(rangeHoursParam) ? rangeHoursParam : 24
    const history = await ResourceMetricsHistoryService.getHistory(rangeHours)

    return response.ok({
      success: true,
      data: history,
    })
  }

  async backupRetentionPolicy({ response }: HttpContext) {
    const policy = await this.backupRetentionPolicyService.getPolicy()

    return response.ok({
      success: true,
      data: this.serializeBackupRetentionPolicy(policy),
    })
  }

  async updateBackupRetentionPolicy(ctx: HttpContext) {
    const { request, response } = ctx
    const payload = await request.validateUsing(updateBackupRetentionPolicyValidator)

    if (!this.backupRetentionPolicyService.isValidPruneCron(payload.pruneCron)) {
      return response.unprocessableEntity({
        success: false,
        message: 'Expressão cron inválida para o prune automático',
      })
    }

    const { policy, changes } = await this.backupRetentionPolicyService.updatePolicy(payload)

    if (Object.keys(changes).length > 0) {
      await AuditService.logSettingsUpdated(changes, ctx)
    }

    await getScheduler().refreshRetentionJob()

    return response.ok({
      success: true,
      message: 'Política de retenção atualizada com sucesso',
      data: this.serializeBackupRetentionPolicy(policy),
    })
  }

  async runBackupRetention({ response }: HttpContext) {
    const result = await getScheduler().runRetentionNow()

    return response.ok({
      success: true,
      message: 'Prune de backups executado com sucesso',
      data: result,
    })
  }

  private serializeBackupRetentionPolicy(
    policy: Awaited<ReturnType<BackupRetentionPolicyService['getPolicy']>>
  ) {
    const defaultPolicy = this.backupRetentionPolicyService.getDefaultPolicy()

    return {
      ...policy,
      defaultPruneCron: defaultPolicy.pruneCron,
    }
  }
}
