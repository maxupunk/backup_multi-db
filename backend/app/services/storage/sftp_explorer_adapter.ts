import type { StorageDestinationConfig } from '#models/storage_destination'
import type { StorageExplorerAdapter } from './storage_explorer_adapter.js'
import type { BucketObjectMetadata, ListObjectsOptions, ListObjectsResult } from './types.js'

/**
 * Adapter para exploração de servidores SFTP.
 */
export class SftpExplorerAdapter implements StorageExplorerAdapter {
  private assertSftpConfig(
    config: StorageDestinationConfig
  ): asserts config is Extract<StorageDestinationConfig, { type: 'sftp' }> {
    if (config.type !== 'sftp') {
      throw new Error(`SftpExplorerAdapter recebeu config de tipo "${config.type}"`)
    }
  }

  private async connect(config: Extract<StorageDestinationConfig, { type: 'sftp' }>) {
    const sftpModule = await import('ssh2-sftp-client')
    const SftpClient = sftpModule.default
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

  private resolvePath(basePath: string | undefined, path: string): string {
    const base = (basePath ?? '').replace(/\/+$/, '') || '.'
    const sub = (path ?? '').replace(/^\/+|\/+$/g, '')
    return sub ? `${base}/${sub}` : base
  }

  async listObjects(
    config: StorageDestinationConfig,
    path: string,
    options: ListObjectsOptions
  ): Promise<ListObjectsResult> {
    this.assertSftpConfig(config)
    const client = await this.connect(config)

    try {
      const remotePath = this.resolvePath(config.basePath, path)
      const listing = await client.list(remotePath)
      const limit = options.limit ?? 100

      const objects: ListObjectsResult['objects'] = []
      let count = 0

      for (const item of listing) {
        if (count >= limit) break
        if (item.name === '.' || item.name === '..') continue

        objects.push({
          key: `${remotePath}/${item.name}`.replace(/\/+/g, '/'),
          name: item.name,
          size: item.type === 'd' ? null : item.size,
          lastModified: item.modifyTime ? new Date(item.modifyTime).toISOString() : null,
          isDirectory: item.type === 'd',
        })
        count++
      }

      return {
        objects,
        nextCursor: null, // SFTP não suporta paginação por cursor
        isTruncated: listing.length > limit,
      }
    } finally {
      await client.end()
    }
  }

  async getObjectMetadata(
    config: StorageDestinationConfig,
    key: string
  ): Promise<BucketObjectMetadata> {
    this.assertSftpConfig(config)
    const client = await this.connect(config)

    try {
      const stat = await client.stat(key)

      return {
        key,
        size: stat.size,
        lastModified: stat.modifyTime ? new Date(stat.modifyTime).toISOString() : null,
        contentType: null, // SFTP não tem content-type
        etag: null,
        metadata: {},
      }
    } finally {
      await client.end()
    }
  }

  async getPresignedUrl(
    _config: StorageDestinationConfig,
    _key: string,
    _expiresInSeconds: number
  ): Promise<string> {
    throw new Error('Presigned URLs não são suportadas para SFTP')
  }

  async deleteObject(
    config: StorageDestinationConfig,
    key: string,
    isDirectory: boolean
  ): Promise<void> {
    this.assertSftpConfig(config)
    const client = await this.connect(config)

    try {
      if (isDirectory) {
        await client.rmdir(key, true)
        return
      }

      await client.delete(key)
    } finally {
      await client.end()
    }
  }

  async testConnection(config: StorageDestinationConfig): Promise<void> {
    this.assertSftpConfig(config)
    const client = await this.connect(config)
    try {
      const remotePath = (config.basePath ?? '').replace(/\/+$/, '') || '.'
      await client.list(remotePath)
    } finally {
      await client.end()
    }
  }
}
