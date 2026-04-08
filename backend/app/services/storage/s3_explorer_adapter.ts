import type { StorageDestinationConfig } from '#models/storage_destination'
import { S3ConfigService } from './s3_config_service.js'
import type { StorageExplorerAdapter } from './storage_explorer_adapter.js'
import type { BucketObjectMetadata, ListObjectsOptions, ListObjectsResult } from './types.js'

/**
 * Adapter para exploração de buckets S3-compatíveis (AWS S3, MinIO, Cloudflare R2).
 * Todos usam @aws-sdk/client-s3 com endpoint customizado quando necessário.
 */
export class S3ExplorerAdapter implements StorageExplorerAdapter {
  private async getClient(config: Extract<StorageDestinationConfig, { type: 's3' }>) {
    const { S3Client } = await import('@aws-sdk/client-s3')
    const normalized = S3ConfigService.normalize(config)

    return new S3Client({
      region: normalized.region,
      endpoint: normalized.endpoint,
      forcePathStyle: normalized.forcePathStyle ?? false,
      credentials: {
        accessKeyId: normalized.accessKeyId,
        secretAccessKey: normalized.secretAccessKey,
      },
    })
  }

  private assertS3Config(
    config: StorageDestinationConfig
  ): asserts config is Extract<StorageDestinationConfig, { type: 's3' }> {
    if (config.type !== 's3') {
      throw new Error(`S3ExplorerAdapter recebeu config de tipo "${config.type}"`)
    }
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
    this.assertS3Config(config)
    const { ListObjectsV2Command } = await import('@aws-sdk/client-s3')

    const client = await this.getClient(config)
    const prefix = this.buildPrefix(config.prefix, path)
    const limit = options.limit ?? 100

    const response = await client.send(
      new ListObjectsV2Command({
        Bucket: config.bucket,
        Prefix: prefix,
        Delimiter: '/',
        MaxKeys: limit,
        ContinuationToken: options.cursor || undefined,
      })
    )

    const objects: ListObjectsResult['objects'] = []

    // Diretórios (prefixos comuns)
    for (const cp of response.CommonPrefixes ?? []) {
      if (!cp.Prefix) continue
      const name = cp.Prefix.replace(prefix, '').replace(/\/$/, '')
      if (!name) continue
      objects.push({
        key: cp.Prefix,
        name,
        size: null,
        lastModified: null,
        isDirectory: true,
      })
    }

    // Arquivos
    for (const item of response.Contents ?? []) {
      if (!item.Key) continue
      // Ignora o próprio prefixo
      if (item.Key === prefix) continue
      const name = item.Key.replace(prefix, '')
      if (!name) continue
      objects.push({
        key: item.Key,
        name,
        size: item.Size ?? 0,
        lastModified: item.LastModified?.toISOString() ?? null,
        isDirectory: false,
        etag: item.ETag?.replace(/"/g, ''),
      })
    }

    return {
      objects,
      nextCursor: response.NextContinuationToken ?? null,
      isTruncated: response.IsTruncated ?? false,
    }
  }

  async getObjectMetadata(
    config: StorageDestinationConfig,
    key: string
  ): Promise<BucketObjectMetadata> {
    this.assertS3Config(config)
    const { HeadObjectCommand } = await import('@aws-sdk/client-s3')
    const client = await this.getClient(config)

    const response = await client.send(
      new HeadObjectCommand({
        Bucket: config.bucket,
        Key: key,
      })
    )

    return {
      key,
      size: response.ContentLength ?? 0,
      lastModified: response.LastModified?.toISOString() ?? null,
      contentType: response.ContentType ?? null,
      etag: response.ETag?.replace(/"/g, '') ?? null,
      metadata: (response.Metadata as Record<string, string>) ?? {},
    }
  }

  async getPresignedUrl(
    config: StorageDestinationConfig,
    key: string,
    expiresInSeconds: number
  ): Promise<string> {
    this.assertS3Config(config)
    const { GetObjectCommand } = await import('@aws-sdk/client-s3')
    const { getSignedUrl } = await import('@aws-sdk/s3-request-presigner')
    const client: any = await this.getClient(config)

    return getSignedUrl(client, new GetObjectCommand({ Bucket: config.bucket, Key: key }), {
      expiresIn: expiresInSeconds,
    })
  }

  async testConnection(config: StorageDestinationConfig): Promise<void> {
    this.assertS3Config(config)
    const { ListObjectsV2Command } = await import('@aws-sdk/client-s3')
    const client = await this.getClient(config)

    await client.send(
      new ListObjectsV2Command({
        Bucket: config.bucket,
        MaxKeys: 1,
      })
    )
  }
}
