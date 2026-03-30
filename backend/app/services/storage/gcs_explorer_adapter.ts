import type { StorageDestinationConfig } from '#models/storage_destination'
import type { StorageExplorerAdapter } from './storage_explorer_adapter.js'
import type { BucketObjectMetadata, ListObjectsOptions, ListObjectsResult } from './types.js'

/**
 * Adapter para exploração de buckets Google Cloud Storage.
 */
export class GcsExplorerAdapter implements StorageExplorerAdapter {
  private assertGcsConfig(
    config: StorageDestinationConfig
  ): asserts config is Extract<StorageDestinationConfig, { type: 'gcs' }> {
    if (config.type !== 'gcs') {
      throw new Error(`GcsExplorerAdapter recebeu config de tipo "${config.type}"`)
    }
  }

  private async getStorage(config: Extract<StorageDestinationConfig, { type: 'gcs' }>) {
    const { Storage } = await import('@google-cloud/storage')
    const options: Record<string, unknown> = {}
    if (config.projectId) options.projectId = config.projectId
    if (config.credentialsJson) {
      options.credentials = JSON.parse(config.credentialsJson)
    }
    return new Storage(options as any)
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
    this.assertGcsConfig(config)
    const storage = await this.getStorage(config)
    const bucket = storage.bucket(config.bucket)
    const prefix = this.buildPrefix(config.prefix, path)
    const limit = options.limit ?? 100

    const [files, , apiResponse] = await bucket.getFiles({
      prefix: prefix || undefined,
      delimiter: '/',
      maxResults: limit,
      pageToken: options.cursor || undefined,
      autoPaginate: false,
    })

    const objects: ListObjectsResult['objects'] = []

    // Diretórios (prefixos)
    const prefixes =
      (apiResponse as Record<string, unknown>)?.prefixes ?? (apiResponse as any)?.['prefixes'] ?? []
    for (const p of prefixes as string[]) {
      const name = p.replace(prefix, '').replace(/\/$/, '')
      if (!name) continue
      objects.push({
        key: p,
        name,
        size: null,
        lastModified: null,
        isDirectory: true,
      })
    }

    // Arquivos
    for (const file of files) {
      if (file.name === prefix) continue
      const name = file.name.replace(prefix, '')
      if (!name || name.endsWith('/')) continue
      objects.push({
        key: file.name,
        name,
        size: Number(file.metadata.size ?? 0),
        lastModified: file.metadata.updated
          ? new Date(file.metadata.updated as string).toISOString()
          : null,
        isDirectory: false,
        etag: (file.metadata.etag as string) ?? undefined,
      })
    }

    const nextPageToken = (apiResponse as any)?.nextPageToken ?? null

    return {
      objects,
      nextCursor: nextPageToken,
      isTruncated: !!nextPageToken,
    }
  }

  async getObjectMetadata(
    config: StorageDestinationConfig,
    key: string
  ): Promise<BucketObjectMetadata> {
    this.assertGcsConfig(config)
    const storage = await this.getStorage(config)
    const file = storage.bucket(config.bucket).file(key)
    const [metadata] = await file.getMetadata()

    return {
      key,
      size: Number(metadata.size ?? 0),
      lastModified: metadata.updated ? new Date(metadata.updated as string).toISOString() : null,
      contentType: (metadata.contentType as string) ?? null,
      etag: (metadata.etag as string) ?? null,
      metadata: (metadata.metadata as Record<string, string>) ?? {},
    }
  }

  async getPresignedUrl(
    config: StorageDestinationConfig,
    key: string,
    expiresInSeconds: number
  ): Promise<string> {
    this.assertGcsConfig(config)
    const storage = await this.getStorage(config)
    const file = storage.bucket(config.bucket).file(key)

    const [url] = await file.getSignedUrl({
      action: 'read',
      expires: Date.now() + expiresInSeconds * 1000,
    })

    return url
  }

  async testConnection(config: StorageDestinationConfig): Promise<void> {
    this.assertGcsConfig(config)
    const storage = await this.getStorage(config)
    await storage.bucket(config.bucket).getFiles({ maxResults: 1 })
  }
}
