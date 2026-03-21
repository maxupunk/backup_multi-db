/**
 * Plugin de integração com AdonisJS Transmit (SSE)
 */
import { Transmit } from '@adonisjs/transmit-client'
import { useNotificationStore } from '@/stores/notification'
import {
  useOperationProgressStore,
  type RestoreProgressEvent,
  type BackupProgressEvent,
} from '@/stores/operation-progress'
import type { App } from 'vue'

// Instância singleton do Transmit
// Usa window.location.origin pois o proxy do Vite redireciona /__transmit para o backend
export const transmit = new Transmit({
  baseUrl: window.location.origin,
})

async function subscribeToChannel(
  channel: string,
  handler: (data: unknown) => void
): Promise<void> {
  try {
    const subscription = transmit.subscription(channel)
    await subscription.create()
    console.log(`[Transmit] Inscrito no canal: ${channel}`)
    subscription.onMessage(handler)
  } catch (error) {
    console.error(`[Transmit] Erro ao se inscrever em ${channel}:`, error)
  }
}

export default {
  install: (app: App) => {
    app.config.globalProperties.$transmit = transmit
    app.provide('transmit', transmit)

    const notificationChannels = [
      'notifications/global',
      'notifications/system',
      'notifications/backup',
      'notifications/storage',
      'notifications/connection',
    ]

    console.log('[Transmit] Inicializando inscrições SSE...')

    notificationChannels.forEach((channel) => {
      subscribeToChannel(channel, (data) => {
        console.log(`[Transmit] Mensagem recebida em ${channel}:`, data)
        const store = useNotificationStore()
        store.add(data as any)
      })
    })

    subscribeToChannel('notifications/restore', (data) => {
      console.log('[Transmit] Progresso de restauração:', data)
      const store = useOperationProgressStore()
      store.handleRestoreProgress(data as RestoreProgressEvent)
    })

    subscribeToChannel('notifications/backup-progress', (data) => {
      console.log('[Transmit] Progresso de backup:', data)
      const store = useOperationProgressStore()
      store.handleBackupProgress(data as BackupProgressEvent)
    })
  },
}
