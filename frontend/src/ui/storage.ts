import type { StorageDestinationType, StorageProvider } from '@/types/api'

export const storageProviderOptions = [
  { title: 'AWS S3', value: 'aws_s3', icon: 'mdi-aws', type: 's3' },
  { title: 'MinIO', value: 'minio', icon: 'mdi-server-network', type: 's3' },
  { title: 'Cloudflare R2', value: 'cloudflare_r2', icon: 'mdi-cloud', type: 's3' },
  { title: 'Google Cloud Storage', value: 'google_gcs', icon: 'mdi-google-cloud', type: 'gcs' },
  { title: 'Azure Blob', value: 'azure_blob', icon: 'mdi-microsoft-azure', type: 'azure_blob' },
  { title: 'SFTP', value: 'sftp', icon: 'mdi-server', type: 'sftp' },
  { title: 'Local', value: 'local', icon: 'mdi-folder', type: 'local' },
] as const

export function getProviderLabel (provider: StorageProvider): string {
  const option = storageProviderOptions.find((o) => o.value === provider)
  return option?.title ?? provider
}

export function getProviderIcon (provider: StorageProvider): string {
  const option = storageProviderOptions.find((o) => o.value === provider)
  return option?.icon ?? 'mdi-bucket'
}

export function getProviderColor (provider: StorageProvider): string {
  const colors: Record<StorageProvider, string> = {
    aws_s3: 'orange-darken-1',
    minio: 'red',
    cloudflare_r2: 'orange',
    google_gcs: 'blue',
    azure_blob: 'light-blue',
    sftp: 'teal',
    local: 'grey',
  }
  return colors[provider] ?? 'grey'
}

export function getTypeForProvider (provider: StorageProvider): StorageDestinationType {
  const option = storageProviderOptions.find((o) => o.value === provider)
  return (option?.type ?? 'local') as StorageDestinationType
}

export function getFileIcon (fileName: string, isDirectory: boolean): string {
  if (isDirectory) return 'mdi-folder'

  const ext = fileName.split('.').pop()?.toLowerCase() ?? ''
  const iconMap: Record<string, string> = {
    sql: 'mdi-database',
    gz: 'mdi-zip-box',
    tar: 'mdi-zip-box',
    zip: 'mdi-zip-box',
    json: 'mdi-code-json',
    csv: 'mdi-file-delimited',
    txt: 'mdi-file-document',
    log: 'mdi-file-document',
    png: 'mdi-file-image',
    jpg: 'mdi-file-image',
    jpeg: 'mdi-file-image',
    gif: 'mdi-file-image',
    pdf: 'mdi-file-pdf-box',
    xml: 'mdi-file-xml-box',
    yml: 'mdi-file-code',
    yaml: 'mdi-file-code',
  }
  return iconMap[ext] ?? 'mdi-file'
}
