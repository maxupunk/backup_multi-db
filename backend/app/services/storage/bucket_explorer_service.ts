import { existsSync } from 'node:fs'
import { DEFAULT_LOCAL_STORAGE_NAME } from '#config/storage_paths'
import Backup from '#models/backup'
import StorageDestination, { type StorageProvider } from '#models/storage_destination'
import { StorageDestinationService } from '#services/storage_destination_service'
import type { StorageExplorerAdapter } from './storage_explorer_adapter.js'
import type {
  BucketObject,
  BucketObjectMetadata,
  BucketObjectReplica,
  ListObjectsOptions,
  ListObjectsResult,
} from './types.js'
import { S3ExplorerAdapter } from './s3_explorer_adapter.js'
import { GcsExplorerAdapter } from './gcs_explorer_adapter.js'
import { AzureExplorerAdapter } from './azure_explorer_adapter.js'
import { SftpExplorerAdapter } from './sftp_explorer_adapter.js'
import { LocalExplorerAdapter } from './local_explorer_adapter.js'

const DEFAULT_PRESIGNED_URL_EXPIRES = 15 * 60 // 15 minutos

/**
 * Serviço de exploração de buckets/storages.
 *
 * Responsabilidade única: listar objetos, obter metadados, excluir itens e gerar URLs
 * pre-assinadas para qualquer storage cadastrado no sistema.
 *
 * O adapter correto é resolvido via factory pelo provider/type do storage.
 */
export class BucketExplorerService {
  private static readonly adapters: Map<string, StorageExplorerAdapter> = new Map()

  private static normalizePath(value: string): string {
    return value
      .replace(/\\/g, '/')
      .trim()
      .replace(/^\.\//, '')
      .replace(/^\/+|\/+$/g, '')
  }

  private static toDbPathVariants(value: string): string[] {
    const normalized = this.normalizePath(value)
    if (!normalized) return []

    const variants = new Set<string>([normalized])
    variants.add(normalized.replace(/\//g, '\\'))
    return [...variants]
  }

  private static toRelativeBackupPath(storage: StorageDestination, objectKey: string): string {
    const normalizedKey = this.normalizePath(objectKey)
    const config = storage.getDecryptedConfig()

    if (!config || config.type === 'local') {
      return normalizedKey
    }

    if (config.type === 's3' || config.type === 'gcs' || config.type === 'azure_blob') {
      const prefix = this.normalizePath(config.prefix ?? '')
      if (!prefix) return normalizedKey
      return normalizedKey.startsWith(`${prefix}/`)
        ? normalizedKey.slice(prefix.length + 1)
        : normalizedKey
    }

    const basePath = this.normalizePath(config.basePath ?? '')
    if (!basePath) return normalizedKey

    return normalizedKey.startsWith(`${basePath}/`)
      ? normalizedKey.slice(basePath.length + 1)
      : normalizedKey
  }

  private static async resolveLocalReplica(): Promise<BucketObjectReplica> {
    const localStorage = await StorageDestination.query()
      .where('type', 'local')
      .where('isDefault', true)
      .orderBy('createdAt', 'asc')
      .first()

    return {
      locationType: 'local',
      storageId: localStorage?.id ?? null,
      storageName: localStorage?.name ?? DEFAULT_LOCAL_STORAGE_NAME,
      provider: 'local',
      path: '',
    }
  }

  private static async enrichObjectsWithReplicas(
    storage: StorageDestination,
    objects: BucketObject[]
  ): Promise<BucketObject[]> {
    const fileEntries = objects
      .filter((object) => !object.isDirectory)
      .map((object) => ({
        object,
        relativePath: this.toRelativeBackupPath(storage, object.key),
      }))
      .filter((entry) => Boolean(entry.relativePath))

    if (fileEntries.length === 0) {
      return objects
    }

    const lookupPaths = [
      ...new Set(fileEntries.flatMap((entry) => this.toDbPathVariants(entry.relativePath))),
    ]
    const backupRows = await Backup.query()
      .where('status', 'completed')
      .whereNotNull('filePath')
      .whereIn('filePath', lookupPaths)
      .preload('storageDestination', (query) => {
        query.select(['id', 'name', 'provider', 'type'])
      })

    const backupsByPath = new Map<string, Backup[]>()
    for (const backup of backupRows) {
      if (!backup.filePath) continue

      const normalizedPath = this.normalizePath(backup.filePath)
      const current = backupsByPath.get(normalizedPath) ?? []
      current.push(backup)
      backupsByPath.set(normalizedPath, current)
    }

    const localReplica = storage.type === 'local' ? null : await this.resolveLocalReplica()

    return objects.map((object) => {
      if (object.isDirectory) {
        return object
      }

      const relativePath = this.toRelativeBackupPath(storage, object.key)
      if (!relativePath) {
        return object
      }

      const replicas: BucketObjectReplica[] = []
      const seenReplicas = new Set<string>()

      if (localReplica) {
        const localPath = StorageDestinationService.getLocalFullPath(null, relativePath)
        if (existsSync(localPath)) {
          const replica = { ...localReplica, path: relativePath }
          replicas.push(replica)
          seenReplicas.add(`local:${replica.storageId ?? 'default'}:${replica.path}`)
        }
      }

      for (const backup of backupsByPath.get(relativePath) ?? []) {
        const destination = backup.storageDestination
        if (!destination) continue
        if (destination.type === 'local') continue
        if (destination.id === storage.id) continue

        const replica: BucketObjectReplica = {
          locationType: 'remote',
          storageId: destination.id,
          storageName: destination.name,
          provider: destination.getEffectiveProvider(),
          path: relativePath,
        }

        const replicaKey = `remote:${replica.storageId}:${replica.path}`
        if (seenReplicas.has(replicaKey)) {
          continue
        }

        replicas.push(replica)
        seenReplicas.add(replicaKey)
      }

      if (replicas.length === 0) {
        return object
      }

      return {
        ...object,
        replicas,
      }
    })
  }

  private static assertDeletableKey(key: string): string {
    const normalizedKey = key.trim()

    if (!normalizedKey || normalizedKey === '/' || normalizedKey === '.') {
      throw new Error('Não é permitido excluir a raiz do armazenamento')
    }

    return normalizedKey
  }

  private static getAdapter(provider: StorageProvider): StorageExplorerAdapter {
    if (this.adapters.has(provider)) {
      return this.adapters.get(provider)!
    }

    let adapter: StorageExplorerAdapter

    switch (provider) {
      case 'aws_s3':
      case 'minio':
      case 'cloudflare_r2':
        adapter = new S3ExplorerAdapter()
        break
      case 'google_gcs':
        adapter = new GcsExplorerAdapter()
        break
      case 'azure_blob':
        adapter = new AzureExplorerAdapter()
        break
      case 'sftp':
        adapter = new SftpExplorerAdapter()
        break
      case 'local':
        adapter = new LocalExplorerAdapter()
        break
      default:
        throw new Error(`Provider não suportado: ${provider}`)
    }

    this.adapters.set(provider, adapter)
    return adapter
  }

  static async listObjects(
    storage: StorageDestination,
    path: string = '',
    options: ListObjectsOptions = {}
  ): Promise<ListObjectsResult> {
    const config = storage.getDecryptedConfig()
    if (!config) {
      throw new Error('Configuração do storage inválida ou ausente')
    }

    const provider = storage.getEffectiveProvider()
    const adapter = this.getAdapter(provider)
    const result = await adapter.listObjects(config, path, options)

    return {
      ...result,
      objects: await this.enrichObjectsWithReplicas(storage, result.objects),
    }
  }

  static async getObjectMetadata(
    storage: StorageDestination,
    key: string
  ): Promise<BucketObjectMetadata> {
    const config = storage.getDecryptedConfig()
    if (!config) {
      throw new Error('Configuração do storage inválida ou ausente')
    }

    const provider = storage.getEffectiveProvider()
    const adapter = this.getAdapter(provider)
    return adapter.getObjectMetadata(config, key)
  }

  static async getPresignedUrl(
    storage: StorageDestination,
    key: string,
    expiresInSeconds: number = DEFAULT_PRESIGNED_URL_EXPIRES
  ): Promise<string> {
    const config = storage.getDecryptedConfig()
    if (!config) {
      throw new Error('Configuração do storage inválida ou ausente')
    }

    const provider = storage.getEffectiveProvider()
    const adapter = this.getAdapter(provider)
    return adapter.getPresignedUrl(config, key, expiresInSeconds)
  }

  static async deleteObject(
    storage: StorageDestination,
    key: string,
    isDirectory: boolean
  ): Promise<void> {
    const config = storage.getDecryptedConfig()
    if (!config) {
      throw new Error('Configuração do storage inválida ou ausente')
    }

    const provider = storage.getEffectiveProvider()
    const adapter = this.getAdapter(provider)
    const normalizedKey = this.assertDeletableKey(key)

    return adapter.deleteObject(config, normalizedKey, isDirectory)
  }

  static async testConnection(storage: StorageDestination): Promise<void> {
    const config = storage.getDecryptedConfig()
    if (!config) {
      throw new Error('Configuração do storage inválida ou ausente')
    }

    const provider = storage.getEffectiveProvider()
    const adapter = this.getAdapter(provider)
    return adapter.testConnection(config)
  }
}
