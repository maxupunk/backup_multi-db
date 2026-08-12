import type { DockerContainerResourceOverview } from '@/types/api'
import { onMounted, onUnmounted, ref, type Ref } from 'vue'
import { subscribe } from '@/services/events'
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
  let unsubscribe: (() => void) | null = null

  async function refresh(): Promise<void> {
    if (loading.value) {
      return
    }

    loading.value = true

    try {
      const response = await systemApi.containerResources()
      overview.value = response
      error.value = null
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Erro ao carregar recursos dos contêineres'
    } finally {
      loading.value = false
    }
  }

  onMounted(async () => {
    await refresh()

    unsubscribe = subscribe(CHANNEL, (data) => {
      overview.value = data as DockerContainerResourceOverview
      error.value = null
      isConnected.value = true
    })

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

    unsubscribe?.()
    unsubscribe = null
    isConnected.value = false
  })

  return {
    overview,
    loading,
    error,
    isConnected,
    refresh,
  }
}
