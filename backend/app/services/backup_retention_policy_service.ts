import cron from 'node-cron'
import SystemSetting from '#models/system_setting'
import env from '#start/env'
import type { RetentionConfig } from '#services/retention_service'

export const DEFAULT_RETENTION_DAILY = 7
export const DEFAULT_RETENTION_WEEKLY = 4
export const DEFAULT_RETENTION_MONTHLY = 12
export const DEFAULT_RETENTION_YEARLY = 5
export const DEFAULT_RETENTION_PRUNE_CRON = '0 2 * * *'
const BACKUP_RETENTION_POLICY_SETTING = 'backup_retention_policy'

export interface BackupRetentionPolicy extends RetentionConfig, Record<string, unknown> {
  pruneCron: string
}

export type BackupRetentionPolicyChanges = Record<string, { from: unknown; to: unknown }>

export class BackupRetentionPolicyService {
  async getPolicy(): Promise<BackupRetentionPolicy> {
    const setting = await SystemSetting.findBy('name', BACKUP_RETENTION_POLICY_SETTING)

    if (!setting) {
      const bootstrapPolicy = this.getDefaultPolicy()

      await SystemSetting.create({
        name: BACKUP_RETENTION_POLICY_SETTING,
        value: bootstrapPolicy,
      })

      return bootstrapPolicy
    }

    const applicationDefaults = this.getApplicationDefaults()
    const storedValue = setting.value

    return {
      daily: this.normalizeCount(storedValue?.daily, applicationDefaults.daily),
      weekly: this.normalizeCount(storedValue?.weekly, applicationDefaults.weekly),
      monthly: this.normalizeCount(storedValue?.monthly, applicationDefaults.monthly),
      yearly: this.normalizeCount(storedValue?.yearly, applicationDefaults.yearly),
      pruneCron: this.normalizeCron(storedValue?.pruneCron, applicationDefaults.pruneCron),
    }
  }

  async updatePolicy(
    payload: BackupRetentionPolicy
  ): Promise<{ policy: BackupRetentionPolicy; changes: BackupRetentionPolicyChanges }> {
    const currentPolicy = await this.getPolicy()
    const nextPolicy: BackupRetentionPolicy = {
      daily: this.normalizeCount(payload.daily, currentPolicy.daily),
      weekly: this.normalizeCount(payload.weekly, currentPolicy.weekly),
      monthly: this.normalizeCount(payload.monthly, currentPolicy.monthly),
      yearly: this.normalizeCount(payload.yearly, currentPolicy.yearly),
      pruneCron: this.normalizeCron(payload.pruneCron, currentPolicy.pruneCron),
    }

    const changes = this.buildChanges(currentPolicy, nextPolicy)

    await SystemSetting.updateOrCreate(
      { name: BACKUP_RETENTION_POLICY_SETTING },
      { value: nextPolicy }
    )

    return {
      policy: nextPolicy,
      changes,
    }
  }

  getDefaultPolicy(): BackupRetentionPolicy {
    const defaults = this.getApplicationDefaults()

    return {
      daily: this.normalizeCount(env.get('RETENTION_DAILY', defaults.daily), defaults.daily),
      weekly: this.normalizeCount(env.get('RETENTION_WEEKLY', defaults.weekly), defaults.weekly),
      monthly: this.normalizeCount(
        env.get('RETENTION_MONTHLY', defaults.monthly),
        defaults.monthly
      ),
      yearly: this.normalizeCount(env.get('RETENTION_YEARLY', defaults.yearly), defaults.yearly),
      pruneCron: this.normalizeCron(
        env.get('RETENTION_PRUNE_CRON', defaults.pruneCron),
        defaults.pruneCron
      ),
    }
  }

  isValidPruneCron(expression: string): boolean {
    return cron.validate(expression.trim())
  }

  private getApplicationDefaults(): BackupRetentionPolicy {
    return {
      daily: DEFAULT_RETENTION_DAILY,
      weekly: DEFAULT_RETENTION_WEEKLY,
      monthly: DEFAULT_RETENTION_MONTHLY,
      yearly: DEFAULT_RETENTION_YEARLY,
      pruneCron: DEFAULT_RETENTION_PRUNE_CRON,
    }
  }

  private buildChanges(
    previous: BackupRetentionPolicy,
    current: BackupRetentionPolicy
  ): BackupRetentionPolicyChanges {
    const changes: BackupRetentionPolicyChanges = {}

    for (const key of Object.keys(current) as Array<keyof BackupRetentionPolicy>) {
      if (previous[key] === current[key]) {
        continue
      }

      changes[key] = {
        from: previous[key],
        to: current[key],
      }
    }

    return changes
  }

  private normalizeCount(value: unknown, fallback: number): number {
    if (typeof value !== 'number' || Number.isNaN(value)) {
      return fallback
    }

    return Math.max(0, Math.trunc(value))
  }

  private normalizeCron(value: unknown, fallback: string): string {
    if (typeof value !== 'string') {
      return fallback
    }

    const trimmedValue = value.trim()

    if (!trimmedValue || !this.isValidPruneCron(trimmedValue)) {
      return fallback
    }

    return trimmedValue
  }
}
