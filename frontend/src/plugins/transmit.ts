/**
 * Plugin de integração com AdonisJS Transmit (SSE)
 */
import { Transmit } from '@adonisjs/transmit-client'
import { useNotificationStore } from '@/stores/notification'
import type { App } from 'vue'

// Instância singleton do Transmit
// Usa window.location.origin pois o proxy do Vite redireciona /__transmit para o backend
export const transmit = new Transmit({
  baseUrl: window.location.origin,
})

export default {
  install: (app: App) => {
    // Injeta $transmit globalmente
    app.config.globalProperties.$transmit = transmit
    app.provide('transmit', transmit)

    // Lista de canais para inscrição automática
    const channels = [
      'notifications/global',
      'notifications/system',
      'notifications/backup',
      'notifications/storage',
      'notifications/connection'
    ]

    console.log('[Transmit] Inicializando inscrições SSE...')

    // Se inscreve nos canais
    channels.forEach(async (channel) => {
      try {
        const subscription = transmit.subscription(channel)
        await subscription.create()
        console.log(`[Transmit] Inscrito no canal: ${channel}`)

        subscription.onMessage((data) => {
          console.log(`[Transmit] Mensagem recebida em ${channel}:`, data)
          const store = useNotificationStore()
          store.add(data as any)
        })
      } catch (error) {
        console.error(`[Transmit] Erro ao se inscrever em ${channel}:`, error)
      }
    })
  }
}
