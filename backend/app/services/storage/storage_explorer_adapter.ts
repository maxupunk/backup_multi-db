import type { StorageDestinationConfig } from '#models/storage_destination'
import type { BucketObjectMetadata, ListObjectsOptions, ListObjectsResult } from './types.js'

/**
 * Interface comum para todos os adapters de exploração de storage.
 * Cada provider implementa esta interface com seu SDK nativo.
 */
export interface StorageExplorerAdapter {
  /**
   * Lista objetos em um caminho específico
   */
  listObjects(
    config: StorageDestinationConfig,
    path: string,
    options: ListObjectsOptions
  ): Promise<ListObjectsResult>

  /**
   * Obtém metadados de um objeto específico
   */
  getObjectMetadata(config: StorageDestinationConfig, key: string): Promise<BucketObjectMetadata>

  /**
   * Gera URL pre-assinada para download (quando suportado)
   */
  getPresignedUrl(
    config: StorageDestinationConfig,
    key: string,
    expiresInSeconds: number
  ): Promise<string>

  /**
   * Testa conectividade com o storage
   */
  testConnection(config: StorageDestinationConfig): Promise<void>
}
