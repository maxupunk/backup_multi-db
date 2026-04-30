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
   * Exclui um arquivo ou pasta do storage.
   * Quando `isDirectory=true`, a implementação deve remover o conteúdo recursivamente.
   */
  deleteObject(config: StorageDestinationConfig, key: string, isDirectory: boolean): Promise<void>

  /**
   * Testa conectividade com o storage
   */
  testConnection(config: StorageDestinationConfig): Promise<void>
}
