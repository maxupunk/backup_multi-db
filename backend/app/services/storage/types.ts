/**
 * Objeto retornado na listagem de um bucket/storage
 */
export interface BucketObject {
  key: string
  name: string
  size: number | null
  lastModified: string | null
  isDirectory: boolean
  etag?: string
}

/**
 * Metadados de um objeto dentro do storage
 */
export interface BucketObjectMetadata {
  key: string
  size: number
  lastModified: string | null
  contentType: string | null
  etag: string | null
  metadata: Record<string, string>
}

/**
 * Opções de listagem de objetos
 */
export interface ListObjectsOptions {
  cursor?: string
  limit?: number
  prefix?: string
}

/**
 * Resultado paginado de listagem de objetos
 */
export interface ListObjectsResult {
  objects: BucketObject[]
  nextCursor: string | null
  isTruncated: boolean
}

/**
 * Resultado de um job de cópia entre storages
 */
export interface CopyJobResult {
  filesTransferred: number
  bytesTransferred: number
  errors: string[]
  duration: number
}

/**
 * Estado de um job de cópia
 */
export interface CopyJob {
  id: string
  sourceStorageId: number
  destinationStorageId: number
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'
  filesTransferred: number
  totalFiles: number | null
  bytesTransferred: number
  error?: string
  startedAt: string
  completedAt?: string
}

/**
 * Estado de um job de archive
 */
export interface ArchiveJob {
  id: string
  storageId: number
  path: string | null
  status: 'pending' | 'building' | 'ready' | 'expired' | 'failed'
  totalFiles: number | null
  processedFiles: number
  error?: string
  startedAt: string
  completedAt?: string
  expiresAt?: string
}

/**
 * Opções para cópia entre storages
 */
export interface CopyOptions {
  sourcePath?: string
  destinationPath?: string
  dryRun?: boolean
  deleteExtraneous?: boolean
}
