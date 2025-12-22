import { DateTime } from 'luxon'
import { BaseModel, beforeSave, column, hasMany } from '@adonisjs/lucid/orm'
import type { HasMany } from '@adonisjs/lucid/types/relations'
import { EncryptionService } from '#services/encryption_service'
import Backup from './backup.js'
import Connection from './connection.js'

export type StorageDestinationType = 'local' | 's3' | 'gcs' | 'azure_blob' | 'sftp'

export type StorageDestinationStatus = 'active' | 'inactive'

export type StorageDestinationConfig =
  | {
      type: 'local'
      basePath?: string
    }
  | {
      type: 's3'
      region: string
      bucket: string
      endpoint?: string
      accessKeyId: string
      secretAccessKey: string
      forcePathStyle?: boolean
      prefix?: string
    }
  | {
      type: 'gcs'
      bucket: string
      projectId?: string
      credentialsJson?: string
      prefix?: string
    }
  | {
      type: 'azure_blob'
      connectionString: string
      container: string
      prefix?: string
    }
  | {
      type: 'sftp'
      host: string
      port?: number
      username: string
      password?: string
      privateKey?: string
      passphrase?: string
      basePath?: string
    }

export default class StorageDestination extends BaseModel {
  @column({ isPrimary: true })
  declare id: number

  @column()
  declare name: string

  @column()
  declare type: StorageDestinationType

  @column()
  declare status: StorageDestinationStatus

  @column()
  declare isDefault: boolean

  @column({ serializeAs: null })
  declare configEncrypted: string

  @column.dateTime({ autoCreate: true })
  declare createdAt: DateTime

  @column.dateTime({ autoCreate: true, autoUpdate: true })
  declare updatedAt: DateTime

  @hasMany(() => Connection)
  declare connections: HasMany<typeof Connection>

  @hasMany(() => Backup)
  declare backups: HasMany<typeof Backup>

  @beforeSave()
  static async encryptConfig(destination: StorageDestination) {
    if (!destination.$dirty.configEncrypted) {
      return
    }

    const currentValue = destination.configEncrypted
    if (!currentValue) {
      return
    }

    if (!EncryptionService.isEncrypted(currentValue)) {
      destination.configEncrypted = EncryptionService.encrypt(currentValue)
    }
  }

  getDecryptedConfig(): StorageDestinationConfig | null {
    if (!this.configEncrypted) return null
    const json = EncryptionService.decrypt(this.configEncrypted)
    return JSON.parse(json)
  }

  setConfig(config: StorageDestinationConfig | null | undefined): void {
    if (!config) {
      this.configEncrypted = ''
      return
    }
    this.configEncrypted = JSON.stringify(config)
  }

  getSafeConfig(): Record<string, unknown> | null {
    const config = this.getDecryptedConfig()
    if (!config) return null

    if (config.type === 's3') {
      const { secretAccessKey, ...rest } = config
      return { ...rest, secretAccessKey: '***' }
    }

    if (config.type === 'gcs') {
      const { credentialsJson, ...rest } = config
      return { ...rest, credentialsJson: credentialsJson ? '***' : undefined }
    }

    if (config.type === 'azure_blob') {
      const { connectionString, ...rest } = config
      return { ...rest, connectionString: '***' }
    }

    if (config.type === 'sftp') {
      const { password, privateKey, passphrase, ...rest } = config
      return {
        ...rest,
        password: password ? '***' : undefined,
        privateKey: privateKey ? '***' : undefined,
        passphrase: passphrase ? '***' : undefined,
      }
    }

    return config
  }
}
