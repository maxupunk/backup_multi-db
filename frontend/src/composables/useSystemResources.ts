import type { MemoryMetricsSource } from '@/types/api'
import { onMounted, onUnmounted, ref, type Ref } from 'vue'
import { subscribe } from '@/services/events'

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
    /** Origem do número: cgroup (dentro de container) ou os.* (fora). */
    source: MemoryMetricsSource
    /** `true` quando há limite de container efetivo aplicado. */
    containerLimited: boolean
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

  let unsubscribe: (() => void) | null = null

  onMounted(() => {
    unsubscribe = subscribe(CHANNEL, (data) => {
      systemResources.value = data as SystemResourcesEvent
      // Só depois do primeiro evento: o backend só coleta métricas quando há
      // alguém ouvindo, então "inscrito" e "recebendo" não são a mesma coisa.
      isConnected.value = true
    })
  })

  onUnmounted(() => {
    unsubscribe?.()
    unsubscribe = null
    isConnected.value = false
  })

  return { systemResources, isConnected }
}
