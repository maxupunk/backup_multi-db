import { createHash } from 'node:crypto'
import type { BlobServiceClient } from '@azure/storage-blob'

export type ReusableAzureBlobConfig = {
  connectionString: string
}

type CacheEntry = {
  client: BlobServiceClient
  lastUsedAt: number
}

export class AzureBlobServiceRegistry {
  private static readonly MAX_CLIENTS = 8
  private static readonly TTL_MS = 15 * 60 * 1000
  private static readonly clients = new Map<string, CacheEntry>()

  static async getClient(config: ReusableAzureBlobConfig): Promise<BlobServiceClient> {
    const now = Date.now()
    const key = this.buildCacheKey(config)

    this.pruneExpiredEntries(now)

    const cached = this.clients.get(key)
    if (cached) {
      cached.lastUsedAt = now
      return cached.client
    }

    const { BlobServiceClient } = await import('@azure/storage-blob')
    const client = BlobServiceClient.fromConnectionString(config.connectionString)

    this.clients.set(key, { client, lastUsedAt: now })
    this.evictOverflowEntries()

    return client
  }

  private static buildCacheKey(config: ReusableAzureBlobConfig): string {
    return createHash('sha256').update(config.connectionString).digest('hex')
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
