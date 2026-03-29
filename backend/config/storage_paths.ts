import env from '#start/env'

export const DEFAULT_BACKUP_STORAGE_PATH = '/app_data/backups'
export const DEFAULT_SQLITE_DATABASE_PATH = '/app_data/database/app.sqlite3'
export const DEFAULT_LOCAL_STORAGE_NAME = 'Local'

export function getBackupStoragePath(): string {
  return env.get('BACKUP_STORAGE_PATH') ?? DEFAULT_BACKUP_STORAGE_PATH
}

export function getSqliteDatabasePath(): string {
  return env.get('SQLITE_DATABASE_PATH') ?? DEFAULT_SQLITE_DATABASE_PATH
}
