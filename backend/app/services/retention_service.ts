import { DateTime } from 'luxon'
import db from '@adonisjs/lucid/services/db'
import Backup from '#models/backup'
import type { BackupStatus, RetentionType } from '#models/backup'
import type StorageDestination from '#models/storage_destination'
import {
  BackupRetentionPlanner,
  type BackupRetentionCandidate,
  type BackupRetentionConfig,
} from '#services/backup_retention_planner'
import { StorageDestinationService } from '#services/storage_destination_service'

const BACKUPS_TABLE = 'backups'

/**
 * Limite conservador de parametros por statement no SQLite.
 */
const ID_BATCH_SIZE = 500

type PrunableBackupRow = {
  id: number
  connection_id: number | null
  connection_database_id: number | null
  database_name: string
  storage_destination_id: number | null
  status: BackupStatus
  retention_type: RetentionType
  file_path: string | null
  file_name: string | null
  created_at: string | number | Date | null
}

/**
 * Projecao enxuta de um backup candidato ao pruning.
 *
 * Carregar modelos Lucid completos custava caro: cada instancia guarda
 * `$attributes`, `$original` e `$dirty` com TODAS as colunas. Numa instalacao
 * com dezenas de milhares de backups, o job de retencao sozinho produzia um
 * pico de dezenas de MB. O planner sempre trabalhou com uma interface simples,
 * entao basta entregar exatamente os campos usados.
 */
type PrunableBackup = BackupRetentionCandidate & {
  storageDestinationId: number | null
  filePath: string | null
  fileName: string | null
}

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
   *
   * O planner precisa do conjunto completo de uma vez (os buckets GFS são
   * calculados globalmente), então o ganho vem de cada item ser pequeno — uma
   * projeção com as 10 colunas usadas, não o modelo Lucid inteiro.
   */
  private async loadPrunableBackups(): Promise<PrunableBackup[]> {
    const rows = (await db
      .from(BACKUPS_TABLE)
      .select(
        'id',
        'connection_id',
        'connection_database_id',
        'database_name',
        'storage_destination_id',
        'status',
        'retention_type',
        'file_path',
        'file_name',
        'created_at'
      )
      .where('protected', false)
      .whereNotIn('status', ['pending', 'running'])
      .orderBy('created_at', 'desc')) as PrunableBackupRow[]

    return rows.map((row) => ({
      id: row.id,
      connectionId: row.connection_id,
      connectionDatabaseId: row.connection_database_id,
      databaseName: row.database_name,
      storageDestinationId: row.storage_destination_id,
      status: row.status,
      retentionType: row.retention_type,
      filePath: row.file_path,
      fileName: row.file_name,
      createdAt: this.parseTimestamp(row.created_at),
    }))
  }

  /**
   * Converte o timestamp cru do SQLite em DateTime.
   *
   * IMPORTANTE: o Lucid grava colunas `@column.dateTime()` no fuso LOCAL, sem
   * marcador de timezone (`2026-08-04 21:26:44`). Interpretar como UTC deslocaria
   * a data pelo offset do servidor e mudaria o bucket GFS do backup — na prática,
   * apagaria backups errados. Por isso `fromSQL` é usado sem `zone`, honrando o
   * fuso local, exatamente como o modelo faz na leitura.
   *
   * Os demais formatos cobrem variações do driver (epoch, Date, ISO).
   */
  private parseTimestamp(value: string | number | Date | null): DateTime {
    if (value instanceof Date) {
      return DateTime.fromJSDate(value)
    }

    if (typeof value === 'number') {
      return DateTime.fromMillis(value)
    }

    if (!value) {
      return DateTime.invalid('timestamp ausente')
    }

    const fromSql = DateTime.fromSQL(value)
    if (fromSql.isValid) {
      return fromSql
    }

    return DateTime.fromISO(value)
  }

  /**
   * Sincroniza o tipo de retenção persistido com o plano calculado em memória.
   *
   * Agrupa por tipo alvo e emite um UPDATE por grupo, em vez de instanciar e
   * salvar um modelo por backup promovido.
   */
  private async syncRetentionTypes(
    backups: PrunableBackup[],
    retainedById: Map<number, RetentionType>
  ): Promise<number> {
    const idsByRetentionType = new Map<RetentionType, number[]>()
    let changed = 0

    for (const backup of backups) {
      const plannedRetention = retainedById.get(backup.id)

      if (!plannedRetention || backup.retentionType === plannedRetention) {
        continue
      }

      const ids = idsByRetentionType.get(plannedRetention) ?? []
      ids.push(backup.id)
      idsByRetentionType.set(plannedRetention, ids)
      changed++
    }

    // Mesmo formato e mesmo fuso (local) que o Lucid usa ao salvar o modelo —
    // gravar em UTC aqui faria a coluna "voltar no tempo" na próxima leitura.
    const updatedAt = DateTime.now().toFormat('yyyy-LL-dd HH:mm:ss')

    for (const [retentionType, ids] of idsByRetentionType) {
      for (const batch of this.chunkIds(ids)) {
        await db
          .from(BACKUPS_TABLE)
          .whereIn('id', batch)
          .update({ retention_type: retentionType, updated_at: updatedAt })
      }
    }

    return changed
  }

  private *chunkIds(ids: number[]): Generator<number[]> {
    for (let index = 0; index < ids.length; index += ID_BATCH_SIZE) {
      yield ids.slice(index, index + ID_BATCH_SIZE)
    }
  }

  /**
   * Deleta uma lista de backups (banco + arquivo físico)
   */
  private async deleteBackups(backups: PrunableBackup[]): Promise<{
    deleted: number
    errors: string[]
    deletedBackups: DeletedBackupSummary[]
  }> {
    let deleted = 0
    const errors: string[] = []
    const deletedBackups: DeletedBackupSummary[] = []

    // Vários backups costumam apontar para o mesmo destino; sem cache seria
    // uma query de StorageDestination por backup excluído.
    const destinationCache = new Map<number, StorageDestination | null>()

    for (const backup of backups) {
      try {
        const summary = this.serializeDeletedBackup(backup)

        // Deletar arquivo físico
        if (backup.filePath) {
          const destination = await this.resolveDestinationCached(backup, destinationCache)
          await StorageDestinationService.deleteBackupFile(destination, backup.filePath)
        }

        // Deletar registro do banco — o arquivo já saiu, então a linha pode ir.
        await db.from(BACKUPS_TABLE).where('id', backup.id).delete()
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

  private async resolveDestinationCached(
    backup: PrunableBackup,
    cache: Map<number, StorageDestination | null>
  ): Promise<StorageDestination | null> {
    if (backup.storageDestinationId === null) {
      return null
    }

    if (cache.has(backup.storageDestinationId)) {
      return cache.get(backup.storageDestinationId) ?? null
    }

    const destination = await StorageDestinationService.resolveDestinationForBackup(backup)
    cache.set(backup.storageDestinationId, destination)

    return destination
  }

  private serializeDeletedBackup(backup: PrunableBackup): DeletedBackupSummary {
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
