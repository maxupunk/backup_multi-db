import Backup from '#models/backup'
import {
  BackupRetentionPlanner,
  type BackupRetentionConfig,
} from '#services/backup_retention_planner'
import { StorageDestinationService } from '#services/storage_destination_service'

/**
 * Configurações de retenção (em unidades)
 */
export interface RetentionConfig extends BackupRetentionConfig {}

export interface DeletedBackupSummary {
  id: number
  connectionId: number | null
  connectionDatabaseId: number | null
  databaseName: string
  fileName: string | null
  retentionType: Backup['retentionType']
  createdAt: string | null
}

export interface RetentionExecutionResult {
  deleted: number
  promoted: number
  protected: number
  errors: string[]
  deletedBackups: DeletedBackupSummary[]
}

/**
 * Serviço responsável pela lógica de retenção GFS (Grandfather-Father-Son) modificada
 */
export class RetentionService {
  private readonly config: RetentionConfig
  private readonly planner: BackupRetentionPlanner

  constructor(config: RetentionConfig) {
    this.config = config

    this.planner = new BackupRetentionPlanner(this.config)
  }

  /**
   * Executa a lógica de pruning (limpeza) de backups antigos
   */
  async pruneBackups(): Promise<RetentionExecutionResult> {
    const errors: string[] = []
    const deletedBackups: DeletedBackupSummary[] = []
    let deleted = 0
    let promoted = 0
    let protectedCount = 0

    try {
      const backups = await this.loadPrunableBackups()
      const plan = this.planner.plan(backups)

      promoted = await this.syncRetentionTypes(backups, plan.retained)

      const backupsToDeleteIds = new Set(plan.toDelete)
      const backupsToDelete = backups.filter((backup) => backupsToDeleteIds.has(backup.id))
      const deletionResult = await this.deleteBackups(backupsToDelete)
      deleted += deletionResult.deleted
      errors.push(...deletionResult.errors)
      deletedBackups.push(...deletionResult.deletedBackups)

      // Contar backups protegidos
      const protectedResult = await Backup.query()
        .where('protected', true)
        .count('* as total')
        .first()
      protectedCount = Number(protectedResult?.$extras.total ?? 0)

      return { deleted, promoted, protected: protectedCount, errors, deletedBackups }
    } catch (error) {
      errors.push(error instanceof Error ? error.message : 'Erro desconhecido no pruning')
      return { deleted, promoted, protected: protectedCount, errors, deletedBackups }
    }
  }

  /**
   * Busca backups que podem participar do pruning automático.
   */
  private async loadPrunableBackups(): Promise<Backup[]> {
    return await Backup.query()
      .where('protected', false)
      .whereNotIn('status', ['pending', 'running'])
      .orderBy('createdAt', 'desc')
  }

  /**
   * Sincroniza o tipo de retenção persistido com o plano calculado em memória.
   */
  private async syncRetentionTypes(
    backups: Backup[],
    retainedById: Map<number, Backup['retentionType']>
  ): Promise<number> {
    let changed = 0

    for (const backup of backups) {
      const plannedRetention = retainedById.get(backup.id)

      if (!plannedRetention || backup.retentionType === plannedRetention) {
        continue
      }

      backup.retentionType = plannedRetention
      await backup.save()
      changed++
    }

    return changed
  }

  /**
   * Deleta uma lista de backups (banco + arquivo físico)
   */
  private async deleteBackups(backups: Backup[]): Promise<{
    deleted: number
    errors: string[]
    deletedBackups: DeletedBackupSummary[]
  }> {
    let deleted = 0
    const errors: string[] = []
    const deletedBackups: DeletedBackupSummary[] = []

    for (const backup of backups) {
      try {
        const summary = this.serializeDeletedBackup(backup)

        // Deletar arquivo físico
        if (backup.filePath) {
          const destination = await StorageDestinationService.resolveDestinationForBackup(backup)
          await StorageDestinationService.deleteBackupFile(destination, backup.filePath)
        }

        // Deletar registro do banco
        await backup.delete()
        deleted++
        deletedBackups.push(summary)
      } catch (error) {
        errors.push(
          `Erro ao deletar backup ${backup.id}: ${error instanceof Error ? error.message : 'Erro desconhecido'}`
        )
      }
    }

    return { deleted, errors, deletedBackups }
  }

  private serializeDeletedBackup(backup: Backup): DeletedBackupSummary {
    return {
      id: backup.id,
      connectionId: backup.connectionId,
      connectionDatabaseId: backup.connectionDatabaseId,
      databaseName: backup.databaseName,
      fileName: backup.fileName,
      retentionType: backup.retentionType,
      createdAt: backup.createdAt?.toISO() ?? null,
    }
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
