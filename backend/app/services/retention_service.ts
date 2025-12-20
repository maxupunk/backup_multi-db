import { DateTime } from 'luxon'
import Backup from '#models/backup'
import { unlink } from 'node:fs/promises'
import { join } from 'node:path'
import app from '@adonisjs/core/services/app'
import { existsSync } from 'node:fs'
import env from '#start/env'

/**
 * Configurações de retenção (em unidades)
 */
interface RetentionConfig {
  daily: number // dias
  weekly: number // semanas
  monthly: number // meses
  yearly: number // anos
}

/**
 * Serviço responsável pela lógica de retenção GFS (Grandfather-Father-Son) modificada
 */
export class RetentionService {
  private readonly config: RetentionConfig

  constructor() {
    this.config = {
      daily: env.get('RETENTION_DAILY', 7),
      weekly: env.get('RETENTION_WEEKLY', 4),
      monthly: env.get('RETENTION_MONTHLY', 12),
      yearly: env.get('RETENTION_YEARLY', 5),
    }
  }

  /**
   * Executa a lógica de pruning (limpeza) de backups antigos
   */
  async pruneBackups(): Promise<{
    deleted: number
    promoted: number
    protected: number
    errors: string[]
  }> {
    const errors: string[] = []
    let deleted = 0
    let promoted = 0
    let protectedCount = 0

    try {
      // 1. Promover backups para níveis superiores de retenção
      const promotionResult = await this.promoteBackups()
      promoted = promotionResult.promoted

      // 2. Deletar backups hourly antigos
      const hourlyResult = await this.cleanupHourlyBackups()
      deleted += hourlyResult.deleted
      errors.push(...hourlyResult.errors)

      // 3. Deletar backups daily antigos
      const dailyResult = await this.cleanupDailyBackups()
      deleted += dailyResult.deleted
      errors.push(...dailyResult.errors)

      // 4. Deletar backups weekly antigos
      const weeklyResult = await this.cleanupWeeklyBackups()
      deleted += weeklyResult.deleted
      errors.push(...weeklyResult.errors)

      // 5. Deletar backups monthly antigos
      const monthlyResult = await this.cleanupMonthlyBackups()
      deleted += monthlyResult.deleted
      errors.push(...monthlyResult.errors)

      // 6. Deletar backups yearly antigos
      const yearlyResult = await this.cleanupYearlyBackups()
      deleted += yearlyResult.deleted
      errors.push(...yearlyResult.errors)

      // Contar backups protegidos
      const protectedResult = await Backup.query()
        .where('protected', true)
        .count('* as total')
        .first()
      protectedCount = Number(protectedResult?.$extras.total ?? 0)

      return { deleted, promoted, protected: protectedCount, errors }
    } catch (error) {
      errors.push(error instanceof Error ? error.message : 'Erro desconhecido no pruning')
      return { deleted, promoted, protected: protectedCount, errors }
    }
  }

  /**
   * Promove backups para níveis superiores de retenção
   */
  private async promoteBackups(): Promise<{ promoted: number }> {
    let promoted = 0

    const now = DateTime.now()

    // Promover para yearly (último backup de dezembro)
    if (now.month === 12 && now.day === 31) {
      const lastBackupsOfYear = await this.getLastBackupsOfPeriod('year')
      for (const backup of lastBackupsOfYear) {
        if (backup.retentionType !== 'yearly') {
          backup.promoteRetention('yearly')
          await backup.save()
          promoted++
        }
      }
    }

    // Promover para monthly (último backup do mês)
    if (now.day === now.daysInMonth) {
      const lastBackupsOfMonth = await this.getLastBackupsOfPeriod('month')
      for (const backup of lastBackupsOfMonth) {
        if (
          backup.retentionType !== 'monthly' &&
          backup.retentionType !== 'yearly'
        ) {
          backup.promoteRetention('monthly')
          await backup.save()
          promoted++
        }
      }
    }

    // Promover para weekly (domingo)
    if (now.weekday === 7) {
      const lastBackupsOfWeek = await this.getLastBackupsOfPeriod('week')
      for (const backup of lastBackupsOfWeek) {
        if (
          backup.retentionType !== 'weekly' &&
          backup.retentionType !== 'monthly' &&
          backup.retentionType !== 'yearly'
        ) {
          backup.promoteRetention('weekly')
          await backup.save()
          promoted++
        }
      }
    }

    // Promover para daily (fim do dia - 23h)
    if (now.hour >= 23) {
      const lastBackupsOfDay = await this.getLastBackupsOfPeriod('day')
      for (const backup of lastBackupsOfDay) {
        if (
          backup.retentionType === 'hourly' &&
          backup.status === 'completed'
        ) {
          backup.promoteRetention('daily')
          await backup.save()
          promoted++
        }
      }
    }

    return { promoted }
  }

  /**
   * Obtém os últimos backups de um período por conexão
   */
  private async getLastBackupsOfPeriod(
    period: 'day' | 'week' | 'month' | 'year'
  ): Promise<Backup[]> {
    const now = DateTime.now()
    let startDate: DateTime

    switch (period) {
      case 'day':
        startDate = now.startOf('day')
        break
      case 'week':
        startDate = now.startOf('week')
        break
      case 'month':
        startDate = now.startOf('month')
        break
      case 'year':
        startDate = now.startOf('year')
        break
    }

    // Buscar último backup bem-sucedido de cada conexão no período
    const backups = await Backup.query()
      .where('status', 'completed')
      .where('createdAt', '>=', startDate.toISO()!)
      .orderBy('createdAt', 'desc')

    // Agrupar por connectionId e manter apenas o mais recente de cada
    const lastByConnection = new Map<number, Backup>()
    for (const backup of backups) {
      if (!lastByConnection.has(backup.connectionId)) {
        lastByConnection.set(backup.connectionId, backup)
      }
    }

    return Array.from(lastByConnection.values())
  }

  /**
   * Limpa backups hourly antigos (mantém apenas do dia atual)
   */
  private async cleanupHourlyBackups(): Promise<{
    deleted: number
    errors: string[]
  }> {
    const today = DateTime.now().startOf('day')

    const oldHourlyBackups = await Backup.query()
      .where('retentionType', 'hourly')
      .where('createdAt', '<', today.toSQL())
      .where('protected', false)

    return this.deleteBackups(oldHourlyBackups)
  }

  /**
   * Limpa backups daily antigos
   */
  private async cleanupDailyBackups(): Promise<{
    deleted: number
    errors: string[]
  }> {
    const cutoffDate = DateTime.now()
      .minus({ days: this.config.daily })
      .startOf('day')

    const oldDailyBackups = await Backup.query()
      .where('retentionType', 'daily')
      .where('createdAt', '<', cutoffDate.toSQL())
      .where('protected', false)

    return this.deleteBackups(oldDailyBackups)
  }

  /**
   * Limpa backups weekly antigos
   */
  private async cleanupWeeklyBackups(): Promise<{
    deleted: number
    errors: string[]
  }> {
    const cutoffDate = DateTime.now()
      .minus({ weeks: this.config.weekly })
      .startOf('week')

    const oldWeeklyBackups = await Backup.query()
      .where('retentionType', 'weekly')
      .where('createdAt', '<', cutoffDate.toSQL())
      .where('protected', false)

    return this.deleteBackups(oldWeeklyBackups)
  }

  /**
   * Limpa backups monthly antigos
   */
  private async cleanupMonthlyBackups(): Promise<{
    deleted: number
    errors: string[]
  }> {
    const cutoffDate = DateTime.now()
      .minus({ months: this.config.monthly })
      .startOf('month')

    const oldMonthlyBackups = await Backup.query()
      .where('retentionType', 'monthly')
      .where('createdAt', '<', cutoffDate.toSQL())
      .where('protected', false)

    return this.deleteBackups(oldMonthlyBackups)
  }

  /**
   * Limpa backups yearly antigos
   */
  private async cleanupYearlyBackups(): Promise<{
    deleted: number
    errors: string[]
  }> {
    const cutoffDate = DateTime.now()
      .minus({ years: this.config.yearly })
      .startOf('year')

    const oldYearlyBackups = await Backup.query()
      .where('retentionType', 'yearly')
      .where('createdAt', '<', cutoffDate.toSQL())
      .where('protected', false)

    return this.deleteBackups(oldYearlyBackups)
  }

  /**
   * Deleta uma lista de backups (banco + arquivo físico)
   */
  private async deleteBackups(
    backups: Backup[]
  ): Promise<{ deleted: number; errors: string[] }> {
    let deleted = 0
    const errors: string[] = []

    for (const backup of backups) {
      try {
        // Deletar arquivo físico
        if (backup.filePath) {
          const fullPath = join(
            app.makePath('storage/backups'),
            backup.filePath
          )

          if (existsSync(fullPath)) {
            await unlink(fullPath)
          }
        }

        // Deletar registro do banco
        await backup.delete()
        deleted++
      } catch (error) {
        errors.push(
          `Erro ao deletar backup ${backup.id}: ${error instanceof Error ? error.message : 'Erro desconhecido'}`
        )
      }
    }

    return { deleted, errors }
  }

  /**
   * Marca um backup como protegido contra pruning
   */
  async protectBackup(backupId: number): Promise<void> {
    const backup = await Backup.find(backupId)
    if (!backup) {
      throw new Error('Backup não encontrado')
    }

    backup.protected = true
    await backup.save()
  }

  /**
   * Remove a proteção de um backup
   */
  async unprotectBackup(backupId: number): Promise<void> {
    const backup = await Backup.find(backupId)
    if (!backup) {
      throw new Error('Backup não encontrado')
    }

    backup.protected = false
    await backup.save()
  }
}
