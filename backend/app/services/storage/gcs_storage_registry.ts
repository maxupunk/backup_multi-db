import { createHash } from 'node:crypto'
import type { Storage } from '@google-cloud/storage'

export type ReusableGcsStorageConfig = {
  projectId?: string
  credentialsJson?: string
}

type CacheEntry = {
  client: Storage
  lastUsedAt: number
}

export class GcsStorageRegistry {
  private static readonly MAX_CLIENTS = 8
  private static readonly TTL_MS = 15 * 60 * 1000
  private static readonly clients = new Map<string, CacheEntry>()

  static async getClient(config: ReusableGcsStorageConfig): Promise<Storage> {
    const now = Date.now()
    const key = this.buildCacheKey(config)

    this.pruneExpiredEntries(now)

    const cached = this.clients.get(key)
    if (cached) {
      cached.lastUsedAt = now
      return cached.client
    }

    const { Storage } = await import('@google-cloud/storage')
    const options: Record<string, unknown> = {}
    if (config.projectId?.trim()) {
      options.projectId = config.projectId.trim()
    }
    if (config.credentialsJson?.trim()) {
      options.credentials = JSON.parse(config.credentialsJson)
    }

    const client = new Storage(options as any)

    this.clients.set(key, { client, lastUsedAt: now })
    this.evictOverflowEntries()

    return client
  }

  private static buildCacheKey(config: ReusableGcsStorageConfig): string {
    return createHash('sha256')
      .update(
        JSON.stringify({
          projectId: config.projectId?.trim() ?? null,
          credentialsJson: config.credentialsJson?.trim() ?? null,
        })
      )
      .digest('hex')
  }

  private static pruneExpiredEntries(now: number): void {
    for (const [key, entry] of this.clients.entries()) {
      if (now - entry.lastUsedAt <= this.TTL_MS) {
        continue
      }

      this.clients.delete(key)
    }
  }

  private static evictOverflowEntries(): void {
    const overflow = this.clients.size - this.MAX_CLIENTS
    if (overflow <= 0) {
      return
    }

    const oldestEntries = [...this.clients.entries()]
      .sort(([, left], [, right]) => left.lastUsedAt - right.lastUsedAt)
      .slice(0, overflow)

    for (const [key] of oldestEntries) {
      this.clients.delete(key)
    }
  }
}
