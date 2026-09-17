import type { DatabaseType } from '@/types/api'

export const databaseTypeOptions = [
  { title: 'MySQL', value: 'mysql' },
  { title: 'MariaDB', value: 'mariadb' },
  { title: 'PostgreSQL', value: 'postgresql' },
] as const

export const defaultDatabasePorts: Record<DatabaseType, number> = {
  mysql: 3306,
  mariadb: 3306,
  postgresql: 5432,
}

export function getDatabaseColor (type: DatabaseType): string {
  const colors: Record<DatabaseType, string> = {
    mysql: 'orange',
    mariadb: 'teal',
    postgresql: 'blue',
  }
  return colors[type] ?? 'grey'
}

export function getDatabaseIcon (_type: DatabaseType): string {
  return 'mdi-database'
}

export function getConnectionStatusColor(status: string | null | undefined): string {
  const colors: Record<string, string> = {
    active: 'success',
    inactive: 'grey',
    error: 'error',
  }
  return colors[status ?? ''] ?? 'grey'
}

export function getConnectionStatusLabel(status: string | null | undefined): string {
  const labels: Record<string, string> = {
    active: 'Ativo',
    inactive: 'Inativo',
    error: 'Erro',
  }
  return labels[status ?? ''] ?? 'Desconhecido'
}

