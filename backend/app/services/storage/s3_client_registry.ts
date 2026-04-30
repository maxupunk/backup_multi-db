import { createHash } from 'node:crypto'
import type { S3Client } from '@aws-sdk/client-s3'

export type ReusableS3ClientConfig = {
  region: string
  endpoint?: string
  forcePathStyle?: boolean
  accessKeyId: string
  secretAccessKey: string
}

type CacheEntry = {
  client: S3Client
  lastUsedAt: number
}

export class S3ClientRegistry {
  private static readonly MAX_CLIENTS = 8
  private static readonly TTL_MS = 15 * 60 * 1000
  private static readonly clients = new Map<string, CacheEntry>()

  static async getClient(config: ReusableS3ClientConfig): Promise<S3Client> {
    const now = Date.now()
    const key = this.buildCacheKey(config)

    this.pruneExpiredEntries(now)

    const cached = this.clients.get(key)
    if (cached) {
      cached.lastUsedAt = now
      return cached.client
    }

    const { S3Client } = await import('@aws-sdk/client-s3')
    const client = new S3Client({
      region: config.region,
      endpoint: config.endpoint,
      forcePathStyle: config.forcePathStyle,
      credentials: {
        accessKeyId: config.accessKeyId,
        secretAccessKey: config.secretAccessKey,
      },
    })

    this.clients.set(key, { client, lastUsedAt: now })
    this.evictOverflowEntries()

    return client
  }

  private static buildCacheKey(config: ReusableS3ClientConfig): string {
    return createHash('sha256')
      .update(
        JSON.stringify({
          region: config.region,
          endpoint: config.endpoint ?? null,
          forcePathStyle: config.forcePathStyle ?? false,
          accessKeyId: config.accessKeyId,
          secretAccessKey: config.secretAccessKey,
        })
      )
      .digest('hex')
  }

  private static pruneExpiredEntries(now: number): void {
    for (const [key, entry] of this.clients.entries()) {
      if (now - entry.lastUsedAt <= this.TTL_MS) {
        continue
      }

      this.destroyEntry(key, entry)
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

    for (const [key, entry] of oldestEntries) {
      this.destroyEntry(key, entry)
    }
  }

  private static destroyEntry(key: string, entry: CacheEntry): void {
    this.clients.delete(key)
    entry.client.destroy()
  }
}
