import type { HttpContext } from '@adonisjs/core/http'
import { DateTime } from 'luxon'
import Backup from '#models/backup'
import Connection from '#models/connection'
import { getScheduler } from '#services/scheduler_service'
import { StorageSpaceService } from '#services/storage_space_service'

type JobsStatusResponse = {
  isRunning: boolean
  activeJobs: number
  status: 'ok' | 'down'
}

export default class SystemController {
  private getJobsStatus(): JobsStatusResponse {
    const schedulerStats = getScheduler().getStats()
    return {
      isRunning: schedulerStats.isRunning,
      activeJobs: schedulerStats.activeJobs,
      status: schedulerStats.isRunning ? 'ok' : 'down',
    }
  }

  async stats({ response }: HttpContext) {
    const today = DateTime.now().startOf('day')
    const [
      totalConnections,
      activeConnections,
      totalBackups,
      backupsToday,
      recentBackups,
      storageSpaces,
    ] = await Promise.all([
      Connection.query().count('* as total').first(),
      Connection.query().where('status', 'active').count('* as total').first(),
      Backup.query().count('* as total').first(),
      Backup.query().where('createdAt', '>=', today.toSQL()).count('* as total').first(),
      Backup.query().preload('connection').orderBy('createdAt', 'desc').limit(5),
      StorageSpaceService.getAllDestinationsSpaceInfo(),
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
        jobs: this.getJobsStatus(),
      },
    })
  }

  async status({ response }: HttpContext) {
    return response.ok({
      success: true,
      data: {
        jobs: this.getJobsStatus(),
      },
    })
  }
}
