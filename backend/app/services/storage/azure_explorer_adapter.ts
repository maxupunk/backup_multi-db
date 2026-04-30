import type { StorageDestinationConfig } from '#models/storage_destination'
import { AzureBlobServiceRegistry } from './azure_blob_service_registry.js'
import type { StorageExplorerAdapter } from './storage_explorer_adapter.js'
import type { BucketObjectMetadata, ListObjectsOptions, ListObjectsResult } from './types.js'

/**
 * Adapter para exploração de containers Azure Blob Storage.
 */
export class AzureExplorerAdapter implements StorageExplorerAdapter {
  private assertAzureConfig(
    config: StorageDestinationConfig
  ): asserts config is Extract<StorageDestinationConfig, { type: 'azure_blob' }> {
    if (config.type !== 'azure_blob') {
      throw new Error(`AzureExplorerAdapter recebeu config de tipo "${config.type}"`)
    }
  }

  private async getContainerClient(
    config: Extract<StorageDestinationConfig, { type: 'azure_blob' }>
  ) {
    const service = await AzureBlobServiceRegistry.getClient({
      connectionString: config.connectionString,
    })
    return service.getContainerClient(config.container)
  }

  private buildPrefix(configPrefix: string | undefined, path: string): string {
    const parts: string[] = []
    if (configPrefix?.trim()) {
      parts.push(configPrefix.trim().replace(/^\/+|\/+$/g, ''))
    }
    if (path?.trim() && path !== '/') {
      parts.push(path.trim().replace(/^\/+|\/+$/g, ''))
    }
    const prefix = parts.join('/')
    return prefix ? `${prefix}/` : ''
  }

  async listObjects(
    config: StorageDestinationConfig,
    path: string,
    options: ListObjectsOptions
  ): Promise<ListObjectsResult> {
    this.assertAzureConfig(config)
    const container = await this.getContainerClient(config)
    const prefix = this.buildPrefix(config.prefix, path)
    const limit = options.limit ?? 100

    const objects: ListObjectsResult['objects'] = []
    const seenDirectories = new Set<string>()
    let count = 0

    // Azure usa listBlobsByHierarchy para simular diretórios
    const iter = container.listBlobsByHierarchy('/', {
      prefix: prefix || undefined,
    })

    for await (const item of iter) {
      if (count >= limit) break

      if (item.kind === 'prefix') {
        const name = item.name.replace(prefix, '').replace(/\/$/, '')
        if (!name || seenDirectories.has(name)) continue
        seenDirectories.add(name)
        objects.push({
          key: item.name,
          name,
          size: null,
          lastModified: null,
          isDirectory: true,
        })
      } else {
        const name = item.name.replace(prefix, '')
        if (!name) continue
        objects.push({
          key: item.name,
          name,
          size: item.properties.contentLength ?? 0,
          lastModified: item.properties.lastModified?.toISOString() ?? null,
          isDirectory: false,
          etag: item.properties.etag?.replace(/"/g, ''),
        })
      }
      count++
    }

    return {
      objects,
      nextCursor: null, // Azure hierárquico não tem paginação por cursor simples
      isTruncated: false,
    }
  }

  async getObjectMetadata(
    config: StorageDestinationConfig,
    key: string
  ): Promise<BucketObjectMetadata> {
    this.assertAzureConfig(config)
    const container = await this.getContainerClient(config)
    const blob = container.getBlobClient(key)
    const properties = await blob.getProperties()

    return {
      key,
      size: properties.contentLength ?? 0,
      lastModified: properties.lastModified?.toISOString() ?? null,
      contentType: properties.contentType ?? null,
      etag: properties.etag?.replace(/"/g, '') ?? null,
      metadata: (properties.metadata as Record<string, string>) ?? {},
    }
  }

  async getPresignedUrl(
    config: StorageDestinationConfig,
    key: string,
    expiresInSeconds: number
  ): Promise<string> {
    this.assertAzureConfig(config)
    const { generateBlobSASQueryParameters, BlobSASPermissions, StorageSharedKeyCredential } =
      await import('@azure/storage-blob')

    const service = await AzureBlobServiceRegistry.getClient({
      connectionString: config.connectionString,
    })
    const container = service.getContainerClient(config.container)
    const blob = container.getBlobClient(key)

    // Extrair credenciais da connection string para gerar SAS
    const match = config.connectionString.match(/AccountName=([^;]+);AccountKey=([^;]+)/)
    if (!match) {
      throw new Error('Não foi possível extrair credenciais da connection string do Azure')
    }

    const credential = new StorageSharedKeyCredential(match[1], match[2])
    const expiresOn = new Date(Date.now() + expiresInSeconds * 1000)

    const sas = generateBlobSASQueryParameters(
      {
        containerName: config.container,
        blobName: key,
        permissions: BlobSASPermissions.parse('r'),
        expiresOn,
      },
      credential
    )

    return `${blob.url}?${sas.toString()}`
  }

  async deleteObject(
    config: StorageDestinationConfig,
    key: string,
    isDirectory: boolean
  ): Promise<void> {
    this.assertAzureConfig(config)
    const container = await this.getContainerClient(config)

    if (!isDirectory) {
      await container.deleteBlob(key)
      return
    }

    const prefix = key.endsWith('/') ? key : `${key}/`

    for await (const blob of container.listBlobsFlat({ prefix })) {
      await container.deleteBlob(blob.name)
    }
  }

  async testConnection(config: StorageDestinationConfig): Promise<void> {
    this.assertAzureConfig(config)
    const container = await this.getContainerClient(config)
    await container.getProperties()
  }
}
