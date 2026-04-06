import { DateTime } from 'luxon'
import ResourceMetricHistory from '#models/resource_metric_history'
import type { DockerContainerResourceOverview } from '#services/docker_container_monitoring_service'
import type { SystemResourceMetrics } from '#services/system_monitoring_service'

export type ResourceHistoryPoint = {
  timestamp: string
  cpuUsagePercent: number
  memoryUsagePercent: number
  memoryUsedBytes: number
  memoryTotalBytes: number
}

export type ContainerResourceHistory = {
  containerId: string
  containerName: string
  points: ResourceHistoryPoint[]
}

export type ResourceMetricsHistoryResponse = {
  retentionDays: number
  system: ResourceHistoryPoint[]
  containers: ContainerResourceHistory[]
}

export class ResourceMetricsHistoryService {
  private static readonly RETENTION_DAYS = 15
  private static readonly MIN_PERSIST_INTERVAL_MS = 60_000
  private static readonly PRUNE_INTERVAL_MS = 6 * 60 * 60 * 1000

  private static readonly lastPersistedAtByKey = new Map<string, number>()
  private static lastPruneAt = 0

  static getRetentionDays(): number {
    return this.RETENTION_DAYS
  }

  static async recordSystemSnapshot(
    resources: SystemResourceMetrics,
    collectedAtIso: string
  ): Promise<void> {
    const key = 'system:global'
    const collectedAt = DateTime.fromISO(collectedAtIso)

    if (!collectedAt.isValid || !this.shouldPersist(key, collectedAt.toMillis())) {
      return
    }

    await ResourceMetricHistory.create({
      scope: 'system',
      entityId: null,
      entityName: 'Servidor',
      cpuUsagePercent: resources.cpu.usagePercent,
      memoryUsagePercent: resources.memory.usagePercent,
      memoryUsedBytes: resources.memory.usedBytes,
      memoryTotalBytes: resources.memory.totalBytes,
      collectedAt,
    })

    await this.pruneOldRecordsIfNeeded()
  }

  static async recordContainerSnapshot(overview: DockerContainerResourceOverview): Promise<void> {
    const collectedAt = DateTime.fromISO(overview.collectedAt)

    if (!overview.dockerAvailable || !collectedAt.isValid) {
      return
    }

    const rows = overview.containers
      .filter((container) =>
        this.shouldPersist(`container:${container.containerId}`, collectedAt.toMillis())
      )
      .map((container) => ({
        scope: 'container' as const,
        entityId: container.containerId,
        entityName: container.containerName,
        cpuUsagePercent: container.cpu.usagePercent,
        memoryUsagePercent: container.memory.usagePercent,
        memoryUsedBytes: container.memory.usageBytes,
        memoryTotalBytes: container.memory.limitBytes,
        collectedAt,
      }))

    if (!rows.length) {
      return
    }

    await ResourceMetricHistory.createMany(rows)
    await this.pruneOldRecordsIfNeeded()
  }

  static async getHistory(rangeHours = 24): Promise<ResourceMetricsHistoryResponse> {
    const boundedHours = Math.max(1, Math.min(rangeHours, this.RETENTION_DAYS * 24))
    const startAt = DateTime.now().minus({ hours: boundedHours })

    const rows = await ResourceMetricHistory.query()
      .where('collected_at', '>=', startAt.toSQL())
      .orderBy('collected_at', 'asc')

    const system: ResourceHistoryPoint[] = []
    const containerMap = new Map<string, ContainerResourceHistory>()

    for (const row of rows) {
      const point: ResourceHistoryPoint = {
        timestamp: row.collectedAt.toISO() ?? DateTime.now().toISO() ?? '',
        cpuUsagePercent: row.cpuUsagePercent,
        memoryUsagePercent: row.memoryUsagePercent,
        memoryUsedBytes: row.memoryUsedBytes,
        memoryTotalBytes: row.memoryTotalBytes,
      }

      if (row.scope === 'system') {
        system.push(point)
        continue
      }

      const containerId = row.entityId ?? 'unknown'
      const existing = containerMap.get(containerId)

      if (existing) {
        existing.points.push(point)
        continue
      }

      containerMap.set(containerId, {
        containerId,
        containerName: row.entityName ?? containerId,
        points: [point],
      })
    }

    return {
      retentionDays: this.RETENTION_DAYS,
      system,
      containers: Array.from(containerMap.values()),
    }
  }

  private static shouldPersist(key: string, collectedAtMs: number): boolean {
    const lastPersistedAt = this.lastPersistedAtByKey.get(key)

    if (lastPersistedAt && collectedAtMs - lastPersistedAt < this.MIN_PERSIST_INTERVAL_MS) {
      return false
    }

    this.lastPersistedAtByKey.set(key, collectedAtMs)
    return true
  }

  private static async pruneOldRecordsIfNeeded(): Promise<void> {
    const nowMs = Date.now()

    if (nowMs - this.lastPruneAt < this.PRUNE_INTERVAL_MS) {
      return
    }

    this.lastPruneAt = nowMs
    const threshold = DateTime.now().minus({ days: this.RETENTION_DAYS })

    await ResourceMetricHistory.query().where('collected_at', '<', threshold.toSQL()).delete()
  }
}
