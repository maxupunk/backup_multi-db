import type { HttpContext } from '@adonisjs/core/http'
import { DateTime } from 'luxon'
import Backup from '#models/backup'
import Connection from '#models/connection'
import { DockerContainerMonitoringService } from '#services/docker_container_monitoring_service'
import { ResourceMetricsHistoryService } from '#services/resource_metrics_history_service'
import { SystemMonitoringService } from '#services/system_monitoring_service'
import { StorageSpaceService } from '#services/storage_space_service'

export default class SystemController {
  private readonly dockerContainerMonitoringService = DockerContainerMonitoringService.instance()

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
}
