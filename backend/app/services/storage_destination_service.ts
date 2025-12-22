import app from '@adonisjs/core/services/app'
import env from '#start/env'
import { existsSync, mkdirSync } from 'node:fs'
import { unlink } from 'node:fs/promises'
import { dirname, join, posix } from 'node:path'
import type { Readable } from 'node:stream'
import StorageDestination from '#models/storage_destination'
import type Backup from '#models/backup'
import type Connection from '#models/connection'

type DownloadResult = {
  stream: Readable
  contentLength?: number
}

export class StorageDestinationService {
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

  static async resolveDestinationForConnection(
    connection: Connection
  ): Promise<StorageDestination | null> {
    if (connection.storageDestinationId) {
      const destination = await StorageDestination.find(connection.storageDestinationId)
      if (destination && destination.status === 'active') return destination
    }

    const defaultDestination = await StorageDestination.query()
      .where('isDefault', true)
      .where('status', 'active')
      .first()

    return defaultDestination ?? null
  }

  static async resolveDestinationForBackup(backup: Backup): Promise<StorageDestination | null> {
    if (!backup.storageDestinationId) return null
    return StorageDestination.find(backup.storageDestinationId)
  }

  static getLocalBasePath(destination: StorageDestination | null): string {
    const fallback = env.get('BACKUP_STORAGE_PATH') ?? app.makePath('storage/backups')
    if (!destination) return fallback

    const config = destination.getDecryptedConfig()
    if (!config) return fallback

    if (config.type === 'local' && config.basePath) {
      return config.basePath
    }

    return fallback
  }

  static getLocalFullPath(destination: StorageDestination | null, relativePath: string): string {
    return join(this.getLocalBasePath(destination), relativePath)
  }

  static ensureLocalDirectory(fullPath: string): void {
    const dir = dirname(fullPath)
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
  }

  static async uploadBackupFile(
    destination: StorageDestination,
    relativePath: string,
    localFullPath: string
  ): Promise<void> {
    const config = destination.getDecryptedConfig()
    if (!config) {
      throw new Error('Configuração do destino de armazenamento inválida')
    }

    if (config.type === 'local') {
      return
    }

    if (config.type === 's3') {
      const { S3Client, PutObjectCommand } = await import('@aws-sdk/client-s3')
      const { createReadStream } = await import('node:fs')

      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const client = new S3Client({
        region: config.region,
        endpoint: config.endpoint,
        forcePathStyle: config.forcePathStyle,
        credentials: {
          accessKeyId: config.accessKeyId,
          secretAccessKey: config.secretAccessKey,
        },
      })

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
      const blob = container.getBlockBlobClient(key)
      await blob.uploadFile(localFullPath)
      return
    }

    if (config.type === 'gcs') {
      const { Storage } = await import('@google-cloud/storage')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)

      const options: Record<string, unknown> = {}
      if (config.projectId) options.projectId = config.projectId
      if (config.credentialsJson) {
        options.credentials = JSON.parse(config.credentialsJson)
      }

      const storage = new Storage(options as any)
      await storage.bucket(config.bucket).upload(localFullPath, { destination: key })
      return
    }

    if (config.type === 'sftp') {
      const sftpModule = await import('ssh2-sftp-client')
      const SftpClient = sftpModule.default
      const key = this.buildRemoteKey('', relativePath)
      const remoteBase = this.normalizePrefix(config.basePath ?? '')
      const remotePath = remoteBase ? posix.join(remoteBase, key) : key
      const remoteDir = posix.dirname(remotePath)

      const client = new SftpClient()
      await client.connect({
        host: config.host,
        port: config.port ?? 22,
        username: config.username,
        password: config.password,
        privateKey: config.privateKey,
        passphrase: config.passphrase,
      } as any)

      try {
        await client.mkdir(remoteDir, true)
        await client.put(localFullPath, remotePath)
      } finally {
        await client.end()
      }
      return
    }
  }

  static async getDownloadStream(
    destination: StorageDestination,
    relativePath: string
  ): Promise<DownloadResult> {
    const config = destination.getDecryptedConfig()
    if (!config) {
      throw new Error('Configuração do destino de armazenamento inválida')
    }

    if (config.type === 'local') {
      const { createReadStream, statSync } = await import('node:fs')
      const fullPath = this.getLocalFullPath(destination, relativePath)
      const stats = statSync(fullPath)
      return { stream: createReadStream(fullPath), contentLength: stats.size }
    }

    if (config.type === 's3') {
      const { S3Client, GetObjectCommand } = await import('@aws-sdk/client-s3')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const client = new S3Client({
        region: config.region,
        endpoint: config.endpoint,
        forcePathStyle: config.forcePathStyle,
        credentials: {
          accessKeyId: config.accessKeyId,
          secretAccessKey: config.secretAccessKey,
        },
      })

      const res = await client.send(
        new GetObjectCommand({
          Bucket: config.bucket,
          Key: key,
        })
      )

      const body = res.Body
      if (!body) {
        throw new Error('Arquivo não encontrado no S3')
      }

      return { stream: body as Readable, contentLength: res.ContentLength }
    }

    if (config.type === 'azure_blob') {
      const { BlobServiceClient } = await import('@azure/storage-blob')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const blobService = BlobServiceClient.fromConnectionString(config.connectionString)
      const container = blobService.getContainerClient(config.container)
      const blob = container.getBlobClient(key)
      const download = await blob.download()
      if (!download.readableStreamBody) {
        throw new Error('Arquivo não encontrado no Azure Blob')
      }
      return {
        stream: download.readableStreamBody as Readable,
        contentLength: download.contentLength,
      }
    }

    if (config.type === 'gcs') {
      const { Storage } = await import('@google-cloud/storage')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)

      const options: Record<string, unknown> = {}
      if (config.projectId) options.projectId = config.projectId
      if (config.credentialsJson) {
        options.credentials = JSON.parse(config.credentialsJson)
      }

      const storage = new Storage(options as any)
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
    if (existsSync(localPath)) {
      await unlink(localPath)
    }

    if (!destination) return

    const config = destination.getDecryptedConfig()
    if (!config || config.type === 'local') return

    if (config.type === 's3') {
      const { S3Client, DeleteObjectCommand } = await import('@aws-sdk/client-s3')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const client = new S3Client({
        region: config.region,
        endpoint: config.endpoint,
        forcePathStyle: config.forcePathStyle,
        credentials: {
          accessKeyId: config.accessKeyId,
          secretAccessKey: config.secretAccessKey,
        },
      })
      await client.send(new DeleteObjectCommand({ Bucket: config.bucket, Key: key }))
      return
    }

    if (config.type === 'azure_blob') {
      const { BlobServiceClient } = await import('@azure/storage-blob')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)
      const blobService = BlobServiceClient.fromConnectionString(config.connectionString)
      const container = blobService.getContainerClient(config.container)
      await container.getBlobClient(key).deleteIfExists()
      return
    }

    if (config.type === 'gcs') {
      const { Storage } = await import('@google-cloud/storage')
      const key = this.buildRemoteKey(config.prefix ?? '', relativePath)

      const options: Record<string, unknown> = {}
      if (config.projectId) options.projectId = config.projectId
      if (config.credentialsJson) {
        options.credentials = JSON.parse(config.credentialsJson)
      }

      const storage = new Storage(options as any)
      await storage.bucket(config.bucket).file(key).delete({ ignoreNotFound: true })
      return
    }

    if (config.type === 'sftp') {
      const sftpModule = await import('ssh2-sftp-client')
      const SftpClient = sftpModule.default
      const key = this.buildRemoteKey('', relativePath)
      const remoteBase = this.normalizePrefix(config.basePath ?? '')
      const remotePath = remoteBase ? posix.join(remoteBase, key) : key

      const client = new SftpClient()
      await client.connect({
        host: config.host,
        port: config.port ?? 22,
        username: config.username,
        password: config.password,
        privateKey: config.privateKey,
        passphrase: config.passphrase,
      } as any)

      try {
        await client.delete(remotePath)
      } finally {
        await client.end()
      }
      return
    }
  }
}
