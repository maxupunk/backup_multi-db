import { DateTime } from 'luxon'
import { BaseModel, belongsTo, column } from '@adonisjs/lucid/orm'
import type { BelongsTo } from '@adonisjs/lucid/types/relations'
import Connection from './connection.js'
import StorageDestination from './storage_destination.js'

/**
 * Status possíveis de um backup
 */
export type BackupStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'

/**
 * Tipos de retenção (GFS - Grandfather-Father-Son)
 */
export type RetentionType = 'hourly' | 'daily' | 'weekly' | 'monthly' | 'yearly'

/**
 * Como o backup foi iniciado
 */
export type BackupTrigger = 'scheduled' | 'manual'

/**
 * Metadados adicionais do backup
 */
export interface BackupMetadata {
  databaseVersion?: string
  tables?: string[]
  [key: string]: unknown
}

/**
 * Model para backups realizados.
 * Armazena informações sobre arquivos de backup, status e retenção.
 */
export default class Backup extends BaseModel {
  @column({ isPrimary: true })
  declare id: number

  @column()
  declare connectionId: number

  @column()
  declare storageDestinationId: number | null

  @column()
  declare status: BackupStatus

  @column()
  declare filePath: string | null

  @column()
  declare fileName: string | null

  @column()
  declare fileSize: number | null

  @column()
  declare checksum: string | null

  @column()
  declare compressed: boolean

  @column()
  declare retentionType: RetentionType

  @column()
  declare protected: boolean

  @column.dateTime()
  declare startedAt: DateTime | null

  @column.dateTime()
  declare finishedAt: DateTime | null

  @column()
  declare durationSeconds: number | null

  @column()
  declare errorMessage: string | null

  @column()
  declare exitCode: number | null

  /**
   * Metadados adicionais serializados como JSON
   */
  @column({
    prepare: (value: BackupMetadata) => (value ? JSON.stringify(value) : null),
    consume: (value: string) => (value ? JSON.parse(value) : null),
  })
  declare metadata: BackupMetadata | null

  @column()
  declare trigger: BackupTrigger

  @column.dateTime({ autoCreate: true })
  declare createdAt: DateTime

  @column.dateTime({ autoCreate: true, autoUpdate: true })
  declare updatedAt: DateTime

  // ==================== Relacionamentos ====================

  @belongsTo(() => Connection)
  declare connection: BelongsTo<typeof Connection>

  @belongsTo(() => StorageDestination)
  declare storageDestination: BelongsTo<typeof StorageDestination>

  // ==================== Métodos de Instância ====================

  /**
   * Verifica se o backup foi bem-sucedido
   */
  isSuccessful(): boolean {
    return this.status === 'completed'
  }

  /**
   * Verifica se o backup está em andamento
   */
  isRunning(): boolean {
    return this.status === 'running' || this.status === 'pending'
  }

  /**
   * Verifica se o backup pode ser deletado (não está protegido)
   */
  canBeDeleted(): boolean {
    return !this.protected && !this.isRunning()
  }

  /**
   * Retorna o tamanho do arquivo formatado
   */
  getFormattedSize(): string {
    if (this.fileSize === null) return 'N/A'

    const units = ['B', 'KB', 'MB', 'GB', 'TB']
    let size = this.fileSize
    let unitIndex = 0

    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024
      unitIndex++
    }

    return `${size.toFixed(2)} ${units[unitIndex]}`
  }

  /**
   * Retorna a duração formatada
   */
  getFormattedDuration(): string {
    if (this.durationSeconds === null) return 'N/A'

    const hours = Math.floor(this.durationSeconds / 3600)
    const minutes = Math.floor((this.durationSeconds % 3600) / 60)
    const seconds = this.durationSeconds % 60

    if (hours > 0) {
      return `${hours}h ${minutes}m ${seconds}s`
    } else if (minutes > 0) {
      return `${minutes}m ${seconds}s`
    } else {
      return `${seconds}s`
    }
  }

  /**
   * Marca o backup como iniciado
   */
  markAsStarted(): void {
    this.status = 'running'
    this.startedAt = DateTime.now()
  }

  /**
   * Marca o backup como concluído com sucesso
   */
  markAsCompleted(filePath: string, fileName: string, fileSize: number, checksum?: string): void {
    this.status = 'completed'
    this.finishedAt = DateTime.now()
    this.filePath = filePath
    this.fileName = fileName
    this.fileSize = fileSize
    this.checksum = checksum ?? null
    this.exitCode = 0

    if (this.startedAt) {
      this.durationSeconds = Math.floor(this.finishedAt.diff(this.startedAt, 'seconds').seconds)
    }
  }

  /**
   * Marca o backup como falho
   */
  markAsFailed(errorMessage: string, exitCode?: number): void {
    this.status = 'failed'
    this.finishedAt = DateTime.now()
    this.errorMessage = errorMessage
    this.exitCode = exitCode ?? null

    if (this.startedAt) {
      this.durationSeconds = Math.floor(this.finishedAt.diff(this.startedAt, 'seconds').seconds)
    }
  }

  /**
   * Promove o backup para um nível de retenção maior
   */
  promoteRetention(newType: RetentionType): void {
    const order: RetentionType[] = ['hourly', 'daily', 'weekly', 'monthly', 'yearly']
    const currentIndex = order.indexOf(this.retentionType)
    const newIndex = order.indexOf(newType)

    if (newIndex > currentIndex) {
      this.retentionType = newType
    }
  }
}
