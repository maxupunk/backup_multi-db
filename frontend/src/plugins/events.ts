/**
 * Liga os canais que o aplicativo inteiro acompanha, sem depender de nenhuma
 * tela estar aberta: notificações e o progresso das operações longas.
 *
 * Os canais de tela — métricas de sistema, recursos de containers, diagnóstico
 * — são assinados pelo próprio componente, e caem quando ele é desmontado. Isso
 * importa: o backend só coleta métricas enquanto houver alguém ouvindo, e
 * assinar tudo aqui manteria a coleta viva para sempre.
 */
import type { App } from 'vue'
import { subscribe } from '@/services/events'
import { useNotificationStore } from '@/stores/notification'
import {
  useOperationProgressStore,
  type BackupProgressEvent,
  type RestoreProgressEvent,
} from '@/stores/operation-progress'

const NOTIFICATION_CHANNELS = [
  'notifications/global',
  'notifications/system',
  'notifications/backup',
  'notifications/storage',
  'notifications/connection',
]

export default {
  install: (_app: App) => {
    for (const channel of NOTIFICATION_CHANNELS) {
      subscribe(channel, (payload) => {
        useNotificationStore().add(payload as never)
      })
    }

    subscribe('notifications/restore-progress', (payload) => {
      useOperationProgressStore().handleRestoreProgress(payload as RestoreProgressEvent)
    })

    subscribe('notifications/backup-progress', (payload) => {
      useOperationProgressStore().handleBackupProgress(payload as BackupProgressEvent)
    })
  },
}
