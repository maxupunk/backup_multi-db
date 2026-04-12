import type StorageDestination from '#models/storage_destination'
import { type StorageProvider } from '#models/storage_destination'
import type { StorageExplorerAdapter } from './storage_explorer_adapter.js'
import type { BucketObjectMetadata, ListObjectsOptions, ListObjectsResult } from './types.js'
import { S3ExplorerAdapter } from './s3_explorer_adapter.js'
import { GcsExplorerAdapter } from './gcs_explorer_adapter.js'
import { AzureExplorerAdapter } from './azure_explorer_adapter.js'
import { SftpExplorerAdapter } from './sftp_explorer_adapter.js'
import { LocalExplorerAdapter } from './local_explorer_adapter.js'

const DEFAULT_PRESIGNED_URL_EXPIRES = 15 * 60 // 15 minutos

/**
 * Serviço de exploração de buckets/storages.
 *
 * Responsabilidade única: listar objetos, obter metadados e gerar URLs pre-assinadas
 * para qualquer storage cadastrado no sistema.
 *
 * O adapter correto é resolvido via factory pelo provider/type do storage.
 */
export class BucketExplorerService {
  private static readonly adapters: Map<string, StorageExplorerAdapter> = new Map()

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
    return adapter.listObjects(config, path, options)
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
