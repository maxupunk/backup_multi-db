import type { BackupStatus } from '@/types/api'

export const backupStatusOptions = [
  { title: 'Pendente', value: 'pending' },
  { title: 'Em execução', value: 'running' },
  { title: 'Concluído', value: 'completed' },
  { title: 'Falhou', value: 'failed' },
  { title: 'Cancelado', value: 'cancelled' },
] as const

export function getBackupStatusColor (status: BackupStatus): string {
  const colors: Record<BackupStatus, string> = {
    pending: 'warning',
    running: 'info',
    completed: 'success',
    failed: 'error',
    cancelled: 'grey',
  }
  return colors[status] ?? 'grey'
}

export function getBackupStatusIcon (status: BackupStatus): string {
  const icons: Record<BackupStatus, string> = {
    pending: 'mdi-clock-outline',
    running: 'mdi-loading mdi-spin',
    completed: 'mdi-check',
    failed: 'mdi-alert-circle',
    cancelled: 'mdi-cancel',
  }
  return icons[status] ?? 'mdi-help'
}

export function getBackupStatusLabel (status: BackupStatus): string {
  const labels: Record<BackupStatus, string> = {
    pending: 'Pendente',
    running: 'Em execução',
    completed: 'Concluído',
    failed: 'Falhou',
    cancelled: 'Cancelado',
  }
  return labels[status] ?? status
}

