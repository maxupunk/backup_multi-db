import { DateTime } from 'luxon'
import db from '@adonisjs/lucid/services/db'
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

type PendingResourceMetricRow = {
  scope: 'system' | 'container'
  entity_id: string | null
  entity_name: string | null
  cpu_usage_percent: number
  memory_usage_percent: number
  memory_used_bytes: number
  memory_total_bytes: number
  collected_at: string
  created_at: string
  updated_at: string
}

export class ResourceMetricsHistoryService {
  private static readonly TABLE_NAME = 'resource_metric_history'
  private static readonly RETENTION_DAYS = 15
  private static readonly MIN_PERSIST_INTERVAL_MS = 60_000
  private static readonly PRUNE_INTERVAL_MS = 6 * 60 * 60 * 1000
  private static readonly FLUSH_INTERVAL_MS = 5 * 60 * 1000
  private static readonly MAX_PENDING_ROWS = 100

  private static readonly lastPersistedAtByKey = new Map<string, number>()
  private static readonly pendingRows: PendingResourceMetricRow[] = []
  private static lastPruneAt = 0
  private static lastFlushAt = Date.now()
  private static flushPromise: Promise<void> | null = null

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

    this.enqueueRows([
      this.buildInsertRow({
        scope: 'system',
        entityId: null,
        entityName: 'Servidor',
        cpuUsagePercent: resources.cpu.usagePercent,
        memoryUsagePercent: resources.memory.usagePercent,
        memoryUsedBytes: resources.memory.usedBytes,
        memoryTotalBytes: resources.memory.totalBytes,
        collectedAt,
      }),
    ])

    await this.flushPendingRowsIfNeeded()
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

    this.enqueueRows(
      rows.map((row) =>
        this.buildInsertRow({
          scope: row.scope,
          entityId: row.entityId,
          entityName: row.entityName,
          cpuUsagePercent: row.cpuUsagePercent,
          memoryUsagePercent: row.memoryUsagePercent,
          memoryUsedBytes: row.memoryUsedBytes,
          memoryTotalBytes: row.memoryTotalBytes,
          collectedAt: row.collectedAt,
        })
      )
    )

    await this.flushPendingRowsIfNeeded()
    await this.pruneOldRecordsIfNeeded()
  }

  static async getHistory(rangeHours = 24): Promise<ResourceMetricsHistoryResponse> {
    const MAX_POINTS = 300
    const boundedHours = Math.max(1, Math.min(rangeHours, this.RETENTION_DAYS * 24))
    const startAt = DateTime.now().minus({ hours: boundedHours })

    await this.flushPendingRows()

    // Bucket rows at the SQL level so we never return more than MAX_POINTS per
    // entity.  This prevents Cloudflare 502 timeouts when fetching long ranges
    // (7d / 15d) that would otherwise produce tens-of-thousands of raw rows.
    const bucketSeconds = Math.max(60, Math.ceil((boundedHours * 3600) / MAX_POINTS))

    type RawRow = {
      scope: 'system' | 'container'
      entity_id: string | null
      entity_name: string | null
      cpu_usage_percent: number
      memory_usage_percent: number
      memory_used_bytes: number
      memory_total_bytes: number
      bucket_time: string
    }

    const rows = (await db
      .from('resource_metric_history')
      .select('scope', 'entity_id', 'entity_name')
      .select(
        db.raw('AVG(cpu_usage_percent) as cpu_usage_percent'),
        db.raw('AVG(memory_usage_percent) as memory_usage_percent'),
        db.raw('CAST(AVG(memory_used_bytes) AS INTEGER) as memory_used_bytes'),
        db.raw('CAST(AVG(memory_total_bytes) AS INTEGER) as memory_total_bytes'),
        db.raw(
          `datetime(CAST(strftime('%s', collected_at) / ? AS INTEGER) * ?, 'unixepoch') as bucket_time`,
          [bucketSeconds, bucketSeconds]
        )
      )
      .where('collected_at', '>=', this.toSqlTimestamp(startAt))
      .groupByRaw(`scope, entity_id, CAST(strftime('%s', collected_at) / ? AS INTEGER)`, [
        bucketSeconds,
      ])
      .orderByRaw('scope, entity_id, bucket_time')) as RawRow[]

    const system: ResourceHistoryPoint[] = []
    const containerMap = new Map<string, ContainerResourceHistory>()

    for (const row of rows) {
      const point: ResourceHistoryPoint = {
        timestamp: row.bucket_time.replace(' ', 'T') + '.000Z',
        cpuUsagePercent: Number(row.cpu_usage_percent),
        memoryUsagePercent: Number(row.memory_usage_percent),
        memoryUsedBytes: Number(row.memory_used_bytes),
        memoryTotalBytes: Number(row.memory_total_bytes),
      }

      if (row.scope === 'system') {
        system.push(point)
        continue
      }

      const containerId = row.entity_id ?? 'unknown'
      const existing = containerMap.get(containerId)

      if (existing) {
        existing.points.push(point)
        continue
      }

      containerMap.set(containerId, {
        containerId,
        containerName: row.entity_name ?? containerId,
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

  static async flushPendingRows(force = true): Promise<void> {
    if (this.flushPromise) {
      await this.flushPromise

      if (force && this.pendingRows.length) {
        await this.flushPendingRows(force)
      }

      return
    }

    if (!this.pendingRows.length) {
      return
    }

    const rowsToInsert = this.pendingRows.splice(0, this.pendingRows.length)

    this.flushPromise = (async () => {
      try {
        await db.table(this.TABLE_NAME).insert(rowsToInsert)
        this.lastFlushAt = Date.now()
      } catch (error) {
        this.pendingRows.unshift(...rowsToInsert)
        throw error
      } finally {
        this.flushPromise = null
      }
    })()

    await this.flushPromise

    if (force && this.pendingRows.length) {
      await this.flushPendingRows(force)
    }
  }

  private static async pruneOldRecordsIfNeeded(): Promise<void> {
    const nowMs = Date.now()

    this.pruneStaleEntries(nowMs)

    if (nowMs - this.lastPruneAt < this.PRUNE_INTERVAL_MS) {
      return
    }

    this.lastPruneAt = nowMs
    const threshold = DateTime.now().minus({ days: this.RETENTION_DAYS })

    await ResourceMetricHistory.query()
      .where('collected_at', '<', this.toSqlTimestamp(threshold))
      .delete()
  }

  private static pruneStaleEntries(nowMs: number): void {
    const staleTtlMs = this.MIN_PERSIST_INTERVAL_MS * 10

    for (const [key, lastPersistedAt] of this.lastPersistedAtByKey.entries()) {
      if (nowMs - lastPersistedAt <= staleTtlMs) {
        continue
      }

      this.lastPersistedAtByKey.delete(key)
    }
  }

  private static enqueueRows(rows: PendingResourceMetricRow[]): void {
    this.pendingRows.push(...rows)
  }

  private static async flushPendingRowsIfNeeded(): Promise<void> {
    if (!this.pendingRows.length) {
      return
    }

    const nowMs = Date.now()
    const shouldFlush =
      this.pendingRows.length >= this.MAX_PENDING_ROWS ||
      nowMs - this.lastFlushAt >= this.FLUSH_INTERVAL_MS

    if (!shouldFlush) {
      return
    }

    await this.flushPendingRows(false)
  }

  private static buildInsertRow(params: {
    scope: PendingResourceMetricRow['scope']
    entityId: string | null
    entityName: string | null
    cpuUsagePercent: number
    memoryUsagePercent: number
    memoryUsedBytes: number
    memoryTotalBytes: number
    collectedAt: DateTime
  }): PendingResourceMetricRow {
    const persistedAt = DateTime.now().toUTC()
    const persistedAtSql = this.toSqlTimestamp(persistedAt)

    return {
      scope: params.scope,
      entity_id: params.entityId,
      entity_name: params.entityName,
      cpu_usage_percent: params.cpuUsagePercent,
      memory_usage_percent: params.memoryUsagePercent,
      memory_used_bytes: params.memoryUsedBytes,
      memory_total_bytes: params.memoryTotalBytes,
      collected_at: this.toSqlTimestamp(params.collectedAt),
      created_at: persistedAtSql,
      updated_at: persistedAtSql,
    }
  }

  private static toSqlTimestamp(value: DateTime): string {
    return value.toUTC().toFormat('yyyy-LL-dd HH:mm:ss')
  }
}
