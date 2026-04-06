import type { DockerContainerResourceOverview } from '@/types/api'
import { onMounted, onUnmounted, ref, type Ref } from 'vue'
import { transmit } from '@/plugins/transmit'
import { systemApi } from '@/services/api'

type UseDockerContainerResourcesOptions = {
  enableFallbackPolling?: boolean
  fallbackIntervalMs?: number
}

type UseDockerContainerResourcesResult = {
  overview: Ref<DockerContainerResourceOverview | null>
  loading: Ref<boolean>
  error: Ref<string | null>
  isConnected: Ref<boolean>
  refresh: () => Promise<void>
}

const CHANNEL = 'notifications/docker-container-resources'
const DEFAULT_FALLBACK_INTERVAL_MS = 10_000

/**
 * Mantém as métricas de recursos dos contêineres Docker atualizadas por SSE.
 * Realiza carga inicial por API e pode usar polling de fallback opcional.
 */
export function useDockerContainerResources(
  options: UseDockerContainerResourcesOptions = {}
): UseDockerContainerResourcesResult {
  const enableFallbackPolling = options.enableFallbackPolling ?? false
  const fallbackIntervalMs = options.fallbackIntervalMs ?? DEFAULT_FALLBACK_INTERVAL_MS

  const overview = ref<DockerContainerResourceOverview | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  const isConnected = ref(false)

  let fallbackIntervalHandle: ReturnType<typeof setInterval> | null = null
  let subscription: ReturnType<typeof transmit.subscription> | null = null

  async function refresh(): Promise<void> {
    if (loading.value) {
      return
    }

    loading.value = true

    try {
      const response = await systemApi.containerResources()
      overview.value = response.data ?? null
      error.value = null
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Erro ao carregar recursos dos contêineres'
    } finally {
      loading.value = false
    }
  }

  onMounted(async () => {
    await refresh()

    try {
      subscription = transmit.subscription(CHANNEL)
      await subscription.create()
      isConnected.value = true

      subscription.onMessage<DockerContainerResourceOverview>((data) => {
        overview.value = data
        error.value = null
      })
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Erro ao conectar no canal de recursos Docker'
      isConnected.value = false
    }

    if (enableFallbackPolling) {
      fallbackIntervalHandle = setInterval(() => {
        void refresh()
      }, fallbackIntervalMs)
    }
  })

  onUnmounted(async () => {
    if (fallbackIntervalHandle) {
      clearInterval(fallbackIntervalHandle)
      fallbackIntervalHandle = null
    }

    if (subscription) {
      try {
        await subscription.delete()
      } catch {
        // noop
      } finally {
        subscription = null
        isConnected.value = false
      }
    }
  })

  return {
    overview,
    loading,
    error,
    isConnected,
    refresh,
  }
}
