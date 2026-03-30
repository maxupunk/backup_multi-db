import { existsSync, statSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { getBackupStoragePath } from '#config/storage_paths'
import type { StorageDestinationConfig } from '#models/storage_destination'
import type { StorageExplorerAdapter } from './storage_explorer_adapter.js'
import type { BucketObjectMetadata, ListObjectsOptions, ListObjectsResult } from './types.js'

/**
 * Adapter para exploração do sistema de arquivos local.
 */
export class LocalExplorerAdapter implements StorageExplorerAdapter {
  private assertLocalConfig(
    config: StorageDestinationConfig
  ): asserts config is Extract<StorageDestinationConfig, { type: 'local' }> {
    if (config.type !== 'local') {
      throw new Error(`LocalExplorerAdapter recebeu config de tipo "${config.type}"`)
    }
  }

  private getBasePath(config: Extract<StorageDestinationConfig, { type: 'local' }>): string {
    return config.basePath?.trim() || getBackupStoragePath()
  }

  private resolveSafePath(basePath: string, subPath: string): string {
    const resolved = join(basePath, subPath)
    // Previne path traversal
    if (!resolved.startsWith(basePath)) {
      throw new Error('Acesso negado: tentativa de path traversal')
    }
    return resolved
  }

  async listObjects(
    config: StorageDestinationConfig,
    path: string,
    options: ListObjectsOptions
  ): Promise<ListObjectsResult> {
    this.assertLocalConfig(config)
    const basePath = this.getBasePath(config)
    const targetPath = this.resolveSafePath(basePath, path || '')
    const limit = options.limit ?? 100

    if (!existsSync(targetPath)) {
      return { objects: [], nextCursor: null, isTruncated: false }
    }

    const entries = readdirSync(targetPath, { withFileTypes: true })
    const objects: ListObjectsResult['objects'] = []
    let count = 0

    for (const entry of entries) {
      if (count >= limit) break

      const fullPath = join(targetPath, entry.name)
      const relativePath = fullPath.replace(basePath, '').replace(/\\/g, '/')
      const isDir = entry.isDirectory()

      let stat = null
      try {
        stat = statSync(fullPath)
      } catch {
        continue
      }

      objects.push({
        key: relativePath,
        name: entry.name,
        size: isDir ? null : stat.size,
        lastModified: stat.mtime.toISOString(),
        isDirectory: isDir,
      })
      count++
    }

    return {
      objects,
      nextCursor: null,
      isTruncated: entries.length > limit,
    }
  }

  async getObjectMetadata(
    config: StorageDestinationConfig,
    key: string
  ): Promise<BucketObjectMetadata> {
    this.assertLocalConfig(config)
    const basePath = this.getBasePath(config)
    const fullPath = this.resolveSafePath(basePath, key)

    if (!existsSync(fullPath)) {
      throw new Error(`Arquivo não encontrado: ${key}`)
    }

    const stat = statSync(fullPath)

    return {
      key,
      size: stat.size,
      lastModified: stat.mtime.toISOString(),
      contentType: null,
      etag: null,
      metadata: {},
    }
  }

  async getPresignedUrl(
    _config: StorageDestinationConfig,
    _key: string,
    _expiresInSeconds: number
  ): Promise<string> {
    throw new Error('Presigned URLs não são suportadas para armazenamento local')
  }

  async testConnection(config: StorageDestinationConfig): Promise<void> {
    this.assertLocalConfig(config)
    const basePath = this.getBasePath(config)
    if (!existsSync(basePath)) {
      throw new Error(`Diretório não encontrado: ${basePath}`)
    }
    // Tenta listar para verificar permissões
    readdirSync(basePath)
  }
}
