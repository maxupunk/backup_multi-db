import { DateTime } from 'luxon'
import { BaseModel, belongsTo, column, hasMany } from '@adonisjs/lucid/orm'
import type { BelongsTo, HasMany } from '@adonisjs/lucid/types/relations'
import Connection from './connection.js'
import Backup from './backup.js'

/**
 * Model para databases associados a uma conexão.
 * Permite que uma conexão tenha múltiplos databases para backup.
 */
export default class ConnectionDatabase extends BaseModel {
  @column({ isPrimary: true })
  declare id: number

  @column()
  declare connectionId: number

  @column()
  declare databaseName: string

  @column()
  declare enabled: boolean

  @column.dateTime({ autoCreate: true })
  declare createdAt: DateTime

  @column.dateTime({ autoCreate: true, autoUpdate: true })
  declare updatedAt: DateTime

  // ==================== Relacionamentos ====================

  @belongsTo(() => Connection)
  declare connection: BelongsTo<typeof Connection>

  @hasMany(() => Backup)
  declare backups: HasMany<typeof Backup>

  // ==================== Métodos de Instância ====================

  /**
   * Verifica se este database está habilitado para backup
   */
  isEnabled(): boolean {
    return this.enabled
  }

  /**
   * Habilita o database para backup
   */
  enable(): void {
    this.enabled = true
  }

  /**
   * Desabilita o database para backup
   */
  disable(): void {
    this.enabled = false
  }
}
