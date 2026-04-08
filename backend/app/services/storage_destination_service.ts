import { createReadStream, existsSync, mkdirSync, statSync } from 'node:fs'
import { unlink } from 'node:fs/promises'
import { dirname, join, posix } from 'node:path'
import type { Readable } from 'node:stream'

import { DEFAULT_LOCAL_STORAGE_NAME, getBackupStoragePath } from '#config/storage_paths'
import type Backup from '#models/backup'
import type Connection from '#models/connection'
import StorageDestination from '#models/storage_destination'
import { S3ConfigService } from '#services/storage/s3_config_service'

type DownloadResult = {
  stream: Readable
  contentLength?: number
}

type S3NormalizedConfig = {
  region: string
  endpoint?: string
  forcePathStyle?: boolean
  accessKeyId: string
  secretAccessKey: string
}

type GcsConfig = {
  projectId?: string
  credentialsJson?: string
}

type SftpConfig = {
  host: string
  port?: number
  username: string
  password?: string
  privateKey?: string
  passphrase?: string
}

export class StorageDestinationService {
  // ---------------------------------------------------------------------------
  // Path helpers
  // ---------------------------------------------------------------------------

  private static normalizePrefix(prefix?: string): string {
    const value = (prefix ?? '').trim()
    if (!value) return ''
    return value.replace(/^\/+|\/+$/g, '')
  }

  private static buildRemoteKey(prefix: string, relativePath: string): string {
    const normalizedPrefix = this.normalizePrefix(prefix)
    const cleanedRelative = relativePath.replace(/^\/+|\/+$/g, '')
    return normalizedPrefix ? posix.join(normalizedPrefix, cleanedRelative) : cleanedRelative
  }

  private static buildSftpRemotePath(basePath: string | undefined, relativePath: string): string {
    const base = this.normalizePrefix(basePath ?? '')
    const key = this.buildRemoteKey('', relativePath)
    return base ? posix.join(base, key) : key
  }

  // ---------------------------------------------------------------------------
  // Provider client factories
  // ---------------------------------------------------------------------------

  private static async createS3Client(config: S3NormalizedConfig) {
    const { S3Client } = await import('@aws-sdk/client-s3')
    return new S3Client({
      region: config.region,
      endpoint: config.endpoint,
      forcePathStyle: config.forcePathStyle,
      credentials: {
        accessKeyId: config.accessKeyId,
        secretAccessKey: config.secretAccessKey,
      },
    })
  }

  private static async createGcsStorage(config: GcsConfig) {
    const { Storage } = await import('@google-cloud/storage')
    const options: Record<string, unknown> = {}
    if (config.projectId) options.projectId = config.projectId
    if (config.credentialsJson) options.credentials = JSON.parse(config.credentialsJson)
    return new Storage(options as any)
  }

  private static async createSftpClient(config: SftpConfig) {
    const { default: SftpClient } = await import('ssh2-sftp-client')
    const client = new SftpClient()
    await client.connect({
      host: config.host,
      port: config.port ?? 22,
      username: config.username,
      password: config.password,
      privateKey: config.privateKey,
      passphrase: config.passphrase,
    } as any)
    return client
  }

  // ---------------------------------------------------------------------------
  // Destination resolution
  // ---------------------------------------------------------------------------

  static async resolveDestinationForConnection(
    connection: Connection
  ): Promise<StorageDestination | null> {
    if (connection.storageDestinationId) {
      const destination = await StorageDestination.find(connection.storageDestinationId)
      if (destination && destination.status === 'active') return destination
    }

    const fallback = await StorageDestination.query()
      .where('isDefault', true)
      .where('status', 'active')
      .first()

    return fallback ?? null
  }

  static async resolveDestinationForBackup(backup: Backup): Promise<StorageDestination | null> {
    if (!backup.storageDestinationId) return null
    return StorageDestination.find(backup.storageDestinationId)
  }

  // ---------------------------------------------------------------------------
  // Local storage helpers
  // ---------------------------------------------------------------------------

  static async ensureDefaultLocalDestination(): Promise<StorageDestination> {
    const fallbackPath = getBackupStoragePath()

    if (!existsSync(fallbackPath)) {
      mkdirSync(fallbackPath, { recursive: true })
    }

    const defaultLocal = await StorageDestination.query()
      .where('type', 'local')
      .where('isDefault', true)
      .first()

    if (defaultLocal) {
      const config = defaultLocal.getDecryptedConfig()
      if (config?.type !== 'local' || !config.basePath?.trim()) {
        defaultLocal.setConfig({ type: 'local', basePath: fallbackPath })
        await defaultLocal.save()
      }
      return defaultLocal
    }

    const existingLocal = await StorageDestination.query()
      .where('type', 'local')
      .where('status', 'active')
      .orderBy('createdAt', 'asc')
      .first()

    if (existingLocal) {
      existingLocal.isDefault = true
      const config = existingLocal.getDecryptedConfig()
      if (config?.type !== 'local' || !config.basePath?.trim()) {
        existingLocal.setConfig({ type: 'local', basePath: fallbackPath })
      }
      await existingLocal.save()
      await StorageDestination.query().whereNot('id', existingLocal.id).update({ isDefault: false })
      return existingLocal
    }

    const destination = await StorageDestination.create({
      name: DEFAULT_LOCAL_STORAGE_NAME,
      type: 'local',
      status: 'active',
      isDefault: true,
      configEncrypted: JSON.stringify({ type: 'local', basePath: fallbackPath }),
    })

    await StorageDestination.query().whereNot('id', destination.id).update({ isDefault: false })

    return destination
  }

  static getLocalBasePath(destination: StorageDestination | null): string {
    const fallback = getBackupStoragePath()
    if (!destination) return fallback

    const config = destination.getDecryptedConfig()
    if (config?.type === 'local' && config.basePath) return config.basePath

    return fallback
  }

  static getLocalFullPath(destination: StorageDestination | null, relativePath: string): string {
    return join(this.getLocalBasePath(destination), relativePath)
  }

  static ensureLocalDirectory(fullPath: string): void {
    const dir = dirname(fullPath)
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
  }

  // ---------------------------------------------------------------------------
  // File operations
  // ---------------------------------------------------------------------------

  static async uploadBackupFile(
    destination: StorageDestination,
    relativePath: string,
    localFullPath: string
  ): Promise<void> {
    const config = destination.getDecryptedConfig()
    if (!config) throw new Error('Configuração do destino de armazenamento inválida')

    if (config.type === 'local') return

    if (config.type === 's3') {
      const { PutObjectCommand } = await import('@aws-sdk/client-s3')
      const normalized = S3ConfigService.normalize(config)
      const key = this.buildRemoteKey(normalized.prefix ?? '', relativePath)
      const client = await this.createS3Client(normalized)
      await client.send(
        new PutObjectCommand({
          Bucket: config.bucket,
          Key: key,
          Body: createReadStream(localFullPath),
        })
      )
      return
    }

    if (config.type === 'azure_blob') {
      const { BlobServiceClient } = await import('@azure/storage-blob')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const blobService = BlobServiceClient.fromConnectionString(config.connectionString)
      const container = blobService.getContainerClient(config.container)
      await container.createIfNotExists()
      await container.getBlockBlobClient(key).uploadFile(localFullPath)
      return
    }

    if (config.type === 'gcs') {
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const storage = await this.createGcsStorage(config)
      await storage.bucket(config.bucket).upload(localFullPath, { destination: key })
      return
    }

    if (config.type === 'sftp') {
      const remotePath = this.buildSftpRemotePath(config.basePath, relativePath)
      const client = await this.createSftpClient(config)
      try {
        await client.mkdir(posix.dirname(remotePath), true)
        await client.put(localFullPath, remotePath)
      } finally {
        await client.end()
      }
    }
  }

  static async getDownloadStream(
    destination: StorageDestination,
    relativePath: string
  ): Promise<DownloadResult> {
    const config = destination.getDecryptedConfig()
    if (!config) throw new Error('Configuração do destino de armazenamento inválida')

    if (config.type === 'local') {
      const fullPath = this.getLocalFullPath(destination, relativePath)
      const stats = statSync(fullPath)
      return { stream: createReadStream(fullPath), contentLength: stats.size }
    }

    if (config.type === 's3') {
      const { GetObjectCommand } = await import('@aws-sdk/client-s3')
      const normalized = S3ConfigService.normalize(config)
      const key = this.buildRemoteKey(normalized.prefix ?? '', relativePath)
      const client = await this.createS3Client(normalized)
      const res = await client.send(new GetObjectCommand({ Bucket: config.bucket, Key: key }))
      if (!res.Body) throw new Error('Arquivo não encontrado no S3')
      return { stream: res.Body as Readable, contentLength: res.ContentLength }
    }

    if (config.type === 'azure_blob') {
      const { BlobServiceClient } = await import('@azure/storage-blob')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const blobService = BlobServiceClient.fromConnectionString(config.connectionString)
      const download = await blobService
        .getContainerClient(config.container)
        .getBlobClient(key)
        .download()
      if (!download.readableStreamBody) throw new Error('Arquivo não encontrado no Azure Blob')
      return {
        stream: download.readableStreamBody as Readable,
        contentLength: download.contentLength,
      }
    }

    if (config.type === 'gcs') {
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const storage = await this.createGcsStorage(config)
      const file = storage.bucket(config.bucket).file(key)
      const [metadata] = await file.getMetadata()
      return { stream: file.createReadStream(), contentLength: Number(metadata.size) }
    }

    throw new Error('Download remoto não suportado para este destino')
  }

  static async deleteBackupFile(
    destination: StorageDestination | null,
    relativePath: string
  ): Promise<void> {
    const localPath = this.getLocalFullPath(destination, relativePath)
    if (existsSync(localPath)) await unlink(localPath)

    if (!destination) return

    const config = destination.getDecryptedConfig()
    if (!config || config.type === 'local') return

    if (config.type === 's3') {
      const { DeleteObjectCommand } = await import('@aws-sdk/client-s3')
      const normalized = S3ConfigService.normalize(config)
      const key = this.buildRemoteKey(normalized.prefix ?? '', relativePath)
      const client = await this.createS3Client(normalized)
      await client.send(new DeleteObjectCommand({ Bucket: config.bucket, Key: key }))
      return
    }

    if (config.type === 'azure_blob') {
      const { BlobServiceClient } = await import('@azure/storage-blob')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const blobService = BlobServiceClient.fromConnectionString(config.connectionString)
      await blobService.getContainerClient(config.container).getBlobClient(key).deleteIfExists()
      return
    }

    if (config.type === 'gcs') {
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const storage = await this.createGcsStorage(config)
      await storage.bucket(config.bucket).file(key).delete({ ignoreNotFound: true })
      return
    }

    if (config.type === 'sftp') {
      const remotePath = this.buildSftpRemotePath(config.basePath, relativePath)
      const client = await this.createSftpClient(config)
      try {
        await client.delete(remotePath)
      } finally {
        await client.end()
      }
    }
  }
}
