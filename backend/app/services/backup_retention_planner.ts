import { DateTime } from 'luxon'
import type { BackupMetadata, BackupStatus, RetentionType } from '#models/backup'

export interface BackupRetentionConfig {
  daily: number
  weekly: number
  monthly: number
  yearly: number
}

export interface BackupRetentionCandidate {
  id: number
  connectionId: number | null
  connectionDatabaseId: number | null
  databaseName: string
  createdAt: DateTime
  status: BackupStatus
  retentionType: RetentionType
  metadata: BackupMetadata | null
}

export interface BackupRetentionPlan {
  retained: Map<number, RetentionType>
  toDelete: number[]
}

/**
 * Planeja a retenção GFS de forma determinística a partir da data de criação do backup,
 * sem depender do horário em que o job de pruning é executado.
 */
export class BackupRetentionPlanner {
  private readonly config: BackupRetentionConfig

  constructor(config: BackupRetentionConfig) {
    this.config = {
      daily: this.normalizeCount(config.daily),
      weekly: this.normalizeCount(config.weekly),
      monthly: this.normalizeCount(config.monthly),
      yearly: this.normalizeCount(config.yearly),
    }
  }

  plan(backups: BackupRetentionCandidate[], now = DateTime.now()): BackupRetentionPlan {
    const retained = new Map<number, RetentionType>()
    const toDelete: number[] = []
    const claimedBuckets = new Set<string>()

    const orderedBackups = [...backups].sort(
      (left, right) => right.createdAt.toMillis() - left.createdAt.toMillis()
    )

    for (const backup of orderedBackups) {
      const tier = this.resolveTier(backup, now)

      if (tier === null) {
        toDelete.push(backup.id)
        continue
      }

      const bucketKey = this.getBucketKey(backup, tier)

      if (claimedBuckets.has(bucketKey)) {
        toDelete.push(backup.id)
        continue
      }

      claimedBuckets.add(bucketKey)
      retained.set(backup.id, tier)
    }

    return { retained, toDelete }
  }

  private resolveTier(backup: BackupRetentionCandidate, now: DateTime): RetentionType | null {
    if (this.isCurrentDay(backup.createdAt, now)) {
      return 'hourly'
    }

    if (backup.status !== 'completed') {
      return null
    }

    if (this.isWithinDailyWindow(backup.createdAt, now)) {
      return 'daily'
    }

    if (this.isWithinWeeklyWindow(backup.createdAt, now)) {
      return 'weekly'
    }

    if (this.isWithinMonthlyWindow(backup.createdAt, now)) {
      return 'monthly'
    }

    if (this.isWithinYearlyWindow(backup.createdAt, now)) {
      return 'yearly'
    }

    return null
  }

  private getBucketKey(backup: BackupRetentionCandidate, tier: RetentionType): string {
    const scopeKey = this.getScopeKey(backup)

    if (backup.status !== 'completed') {
      return `${scopeKey}:terminal:${backup.id}`
    }

    switch (tier) {
      case 'hourly':
        return `${scopeKey}:hourly:${backup.createdAt.toFormat('yyyy-MM-dd-HH')}`
      case 'daily':
        return `${scopeKey}:daily:${backup.createdAt.toISODate()}`
      case 'weekly':
        return `${scopeKey}:weekly:${backup.createdAt.weekYear}-${backup.createdAt.weekNumber}`
      case 'monthly':
        return `${scopeKey}:monthly:${backup.createdAt.toFormat('yyyy-MM')}`
      case 'yearly':
        return `${scopeKey}:yearly:${backup.createdAt.toFormat('yyyy')}`
    }
  }

  private getScopeKey(backup: BackupRetentionCandidate): string {
    if (backup.connectionDatabaseId !== null) {
      return `connection-database:${backup.connectionDatabaseId}`
    }

    if (backup.connectionId !== null) {
      return `connection:${backup.connectionId}:database:${backup.databaseName}`
    }

    return `backup:${backup.id}`
  }

  private isCurrentDay(createdAt: DateTime, now: DateTime): boolean {
    return createdAt >= now.startOf('day')
  }

  private isWithinDailyWindow(createdAt: DateTime, now: DateTime): boolean {
    if (this.config.daily === 0) {
      return false
    }

    return createdAt >= now.minus({ days: this.config.daily }).startOf('day')
  }

  private isWithinWeeklyWindow(createdAt: DateTime, now: DateTime): boolean {
    if (this.config.weekly === 0) {
      return false
    }

    return createdAt >= now.minus({ weeks: this.config.weekly }).startOf('week')
  }

  private isWithinMonthlyWindow(createdAt: DateTime, now: DateTime): boolean {
    if (this.config.monthly === 0) {
      return false
    }

    return createdAt >= now.minus({ months: this.config.monthly }).startOf('month')
  }

  private isWithinYearlyWindow(createdAt: DateTime, now: DateTime): boolean {
    if (this.config.yearly === 0) {
      return false
    }

    return createdAt >= now.minus({ years: this.config.yearly }).startOf('year')
  }

  private normalizeCount(value: number): number {
    return Math.max(0, Math.trunc(value))
  }
}
