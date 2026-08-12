import type { BackupStatus } from '@/types/api'

export const backupStatusOptions = [
  { title: 'Pendente', value: 'pending' },
  { title: 'Em execução', value: 'running' },
  { title: 'Concluído', value: 'completed' },
  { title: 'Falhou', value: 'failed' },
  { title: 'Cancelado', value: 'cancelled' },
] as const

export function getBackupStatusColor (status: string): string {
  const colors: Record<string, string> = {
    pending: 'warning',
    running: 'info',
    completed: 'success',
    failed: 'error',
    cancelled: 'grey',
  }
  return colors[status] ?? 'grey'
}

export function getBackupStatusIcon (status: string): string {
  const icons: Record<string, string> = {
    pending: 'mdi-clock-outline',
    running: 'mdi-loading mdi-spin',
    completed: 'mdi-check',
    failed: 'mdi-alert-circle',
    cancelled: 'mdi-cancel',
  }
  return icons[status] ?? 'mdi-help'
}

export function getBackupStatusLabel (status: string): string {
  const labels: Record<string, string> = {
    pending: 'Pendente',
    running: 'Em execução',
    completed: 'Concluído',
    failed: 'Falhou',
    cancelled: 'Cancelado',
  }
  return labels[status] ?? status
}
