import { onMounted, onUnmounted, ref, type Ref } from 'vue'
import { transmit } from '@/plugins/transmit'

/**
 * Evento de recursos do sistema recebido via SSE
 */
export interface SystemResourcesEvent {
  cpu: {
    usagePercent: number
    cores: number
    model: string
  }
  memory: {
    totalBytes: number
    usedBytes: number
    freeBytes: number
    usagePercent: number
  }
  timestamp: string
}

const CHANNEL = 'notifications/system-resources'

/**
 * Composable que mantém métricas de CPU e RAM atualizadas em tempo real via SSE.
 *
 * - Assina o canal `notifications/system-resources` ao montar o componente.
 * - Cancela a assinatura e limpa recursos ao desmontar (evita memory leaks).
 * - Expõe `systemResources` como ref reativa e `isConnected` para feedback de UI.
 *
 * @example
 * const { systemResources, isConnected } = useSystemResources()
 */
export function useSystemResources(): {
  systemResources: Ref<SystemResourcesEvent | null>
  isConnected: Ref<boolean>
} {
  const systemResources = ref<SystemResourcesEvent | null>(null)
  const isConnected = ref(false)

  let subscription: ReturnType<typeof transmit.subscription> | null = null

  onMounted(async () => {
    try {
      subscription = transmit.subscription(CHANNEL)
      await subscription.create()
      isConnected.value = true

      subscription.onMessage<SystemResourcesEvent>((data) => {
        systemResources.value = data
      })
    } catch (error) {
      console.error('[useSystemResources] Erro ao se inscrever no canal SSE:', error)
    }
  })

  onUnmounted(async () => {
    if (subscription) {
      try {
        await subscription.delete()
      } catch (error) {
        console.error('[useSystemResources] Erro ao cancelar inscrição SSE:', error)
      } finally {
        subscription = null
        isConnected.value = false
      }
    }
  })

  return { systemResources, isConnected }
}
