import type { StorageProvider } from '#models/storage_destination'

type S3ConfigInput = {
  region?: string
  endpoint?: string
  forcePathStyle?: boolean
}

type S3NormalizeOptions = {
  provider?: StorageProvider | null
}

/**
 * Regras de normalizacao para S3-compatibles.
 * - Cloudflare R2 usa regiao "auto" por padrao.
 * - Demais providers usam "us-east-1" quando regiao nao for informada.
 */
export class S3ConfigService {
  private static isR2Endpoint(endpoint?: string): boolean {
    if (!endpoint?.trim()) return false
    return endpoint.includes('.r2.cloudflarestorage.com')
  }

  static resolveRegion(config: S3ConfigInput, options?: S3NormalizeOptions): string {
    if (config.region?.trim()) return config.region.trim()

    const isCloudflareR2 =
      options?.provider === 'cloudflare_r2' || this.isR2Endpoint(config.endpoint)

    if (isCloudflareR2) return 'auto'
    return 'us-east-1'
  }

  static normalize<T extends S3ConfigInput>(
    config: T,
    options?: S3NormalizeOptions
  ): Omit<T, 'region' | 'endpoint'> & { region: string; endpoint?: string } {
    const endpoint = config.endpoint?.trim() ? config.endpoint.trim() : undefined

    return {
      ...config,
      endpoint,
      region: this.resolveRegion({ ...config, endpoint }, options),
    }
  }
}
