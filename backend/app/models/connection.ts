import { DateTime } from 'luxon'
import { BaseModel, beforeSave, column, hasMany } from '@adonisjs/lucid/orm'
import type { HasMany } from '@adonisjs/lucid/types/relations'
import { EncryptionService } from '#services/encryption_service'
import Backup from './backup.js'

/**
 * Tipos de banco de dados suportados
 */
export type DatabaseType = 'mysql' | 'mariadb' | 'postgresql'

/**
 * Frequências de agendamento disponíveis
 */
export type ScheduleFrequency = '1h' | '6h' | '12h' | '24h'

/**
 * Status da conexão
 */
export type ConnectionStatus = 'active' | 'inactive' | 'error'

/**
 * Opções adicionais de conexão
 */
export interface ConnectionOptions {
  ssl?: boolean
  charset?: string
  [key: string]: unknown
}

/**
 * Model para conexões de banco de dados.
 * Gerencia credenciais criptografadas e configurações de backup.
 */
export default class Connection extends BaseModel {
  @column({ isPrimary: true })
  declare id: number

  @column()
  declare name: string

  @column()
  declare type: DatabaseType

  @column()
  declare host: string

  @column()
  declare port: number

  @column()
  declare database: string

  @column()
  declare username: string

  /**
   * Senha criptografada com AES-256-GCM.
   * Formato armazenado: iv:authTag:encryptedData (base64)
   */
  @column({ serializeAs: null }) // Nunca serializar a senha
  declare passwordEncrypted: string

  @column()
  declare scheduleFrequency: ScheduleFrequency | null

  @column()
  declare scheduleEnabled: boolean

  @column()
  declare status: ConnectionStatus

  @column()
  declare lastError: string | null

  @column.dateTime()
  declare lastTestedAt: DateTime | null

  @column.dateTime()
  declare lastBackupAt: DateTime | null

  /**
   * Opções adicionais serializadas como JSON
   */
  @column({
    prepare: (value: ConnectionOptions) => (value ? JSON.stringify(value) : null),
    consume: (value: string) => (value ? JSON.parse(value) : null),
  })
  declare options: ConnectionOptions | null

  @column.dateTime({ autoCreate: true })
  declare createdAt: DateTime

  @column.dateTime({ autoCreate: true, autoUpdate: true })
  declare updatedAt: DateTime

  // ==================== Relacionamentos ====================

  @hasMany(() => Backup)
  declare backups: HasMany<typeof Backup>

  // ==================== Hooks ====================

  /**
   * Antes de salvar, criptografa a senha se ela foi modificada.
   * Verifica se a senha já está criptografada para evitar dupla criptografia.
   */
  @beforeSave()
  static async encryptPassword(connection: Connection) {
    if (connection.$dirty.passwordEncrypted) {
      const currentValue = connection.passwordEncrypted

      // Não criptografar senha vazia
      if (!currentValue) {
        return
      }

      // Evitar dupla criptografia
      if (!EncryptionService.isEncrypted(currentValue)) {
        connection.passwordEncrypted = EncryptionService.encrypt(currentValue)
      }
    }
  }

  // ==================== Métodos de Instância ====================

  /**
   * Obtém a senha descriptografada.
   * ATENÇÃO: Use com cuidado, evite logar ou expor este valor.
   */
  getDecryptedPassword(): string {
    if (!this.passwordEncrypted) return ''
    return EncryptionService.decrypt(this.passwordEncrypted)
  }

  /**
   * Define a senha (será criptografada automaticamente no beforeSave)
   */
  setPassword(plainPassword: string | undefined | null): void {
    this.passwordEncrypted = plainPassword || ''
  }

  /**
   * Retorna a porta padrão baseada no tipo do banco
   */
  static getDefaultPort(type: DatabaseType): number {
    const ports: Record<DatabaseType, number> = {
      mysql: 3306,
      mariadb: 3306,
      postgresql: 5432,
    }
    return ports[type]
  }

  /**
   * Retorna o comando de dump baseado no tipo do banco
   */
  getDumpCommand(): string {
    const commands: Record<DatabaseType, string> = {
      mysql: 'mysqldump',
      mariadb: 'mysqldump',
      postgresql: 'pg_dump',
    }
    return commands[this.type]
  }

  /**
   * Converte frequência para milissegundos
   */
  getScheduleIntervalMs(): number | null {
    if (!this.scheduleFrequency) return null

    const intervals: Record<ScheduleFrequency, number> = {
      '1h': 60 * 60 * 1000,
      '6h': 6 * 60 * 60 * 1000,
      '12h': 12 * 60 * 60 * 1000,
      '24h': 24 * 60 * 60 * 1000,
    }
    return intervals[this.scheduleFrequency]
  }
}
