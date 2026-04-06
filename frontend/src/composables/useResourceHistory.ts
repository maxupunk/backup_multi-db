import type {
  ContainerResourceHistory,
  DockerContainerResourceOverview,
  ResourceHistoryPoint,
  ResourceMetricsHistoryResponse,
} from '@/types/api'
import { computed, ref, type ComputedRef, type Ref } from 'vue'
import { systemApi } from '@/services/api'
import type { SystemResourcesEvent } from './useSystemResources'

type ContainerHistoryById = Record<string, ContainerResourceHistory>

type UseResourceHistoryResult = {
  systemHistory: Ref<ResourceHistoryPoint[]>
  containerHistoryById: ComputedRef<ContainerHistoryById>
  retentionDays: Ref<number>
  loading: Ref<boolean>
  error: Ref<string | null>
  load: (rangeHours?: number) => Promise<void>
  appendSystemEvent: (event: SystemResourcesEvent) => void
  appendContainerOverview: (overview: DockerContainerResourceOverview) => void
}

const MAX_VISIBLE_POINTS = 240

export function useResourceHistory(): UseResourceHistoryResult {
  const systemHistory = ref<ResourceHistoryPoint[]>([])
  const containerHistory = ref<ContainerResourceHistory[]>([])
  const retentionDays = ref(15)
  const loading = ref(false)
  const error = ref<string | null>(null)

  const containerHistoryById = computed<ContainerHistoryById>(() => {
    return containerHistory.value.reduce<ContainerHistoryById>((acc, item) => {
      acc[item.containerId] = item
      return acc
    }, {})
  })

  async function load(rangeHours = 24): Promise<void> {
    loading.value = true

    try {
      const response = await systemApi.resourcesHistory(rangeHours)
      const data = response.data ?? ({} as ResourceMetricsHistoryResponse)

      retentionDays.value = data.retentionDays ?? 15
      systemHistory.value = keepTailPoints(data.system ?? [])
      containerHistory.value = (data.containers ?? []).map((container) => ({
        ...container,
        points: keepTailPoints(container.points ?? []),
      }))
      error.value = null
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Erro ao carregar histórico de recursos'
    } finally {
      loading.value = false
    }
  }

  function appendSystemEvent(event: SystemResourcesEvent): void {
    systemHistory.value = keepTailPoints([
      ...systemHistory.value,
      {
        timestamp: event.timestamp,
        cpuUsagePercent: event.cpu.usagePercent,
        memoryUsagePercent: event.memory.usagePercent,
        memoryUsedBytes: event.memory.usedBytes,
        memoryTotalBytes: event.memory.totalBytes,
      },
    ])
  }

  function appendContainerOverview(overview: DockerContainerResourceOverview): void {
    if (!overview.dockerAvailable) {
      return
    }

    const collectedAt = overview.collectedAt
    const map = new Map(containerHistory.value.map((item) => [item.containerId, item] as const))

    for (const container of overview.containers) {
      const point: ResourceHistoryPoint = {
        timestamp: collectedAt,
        cpuUsagePercent: container.cpu.usagePercent,
        memoryUsagePercent: container.memory.usagePercent,
        memoryUsedBytes: container.memory.usageBytes,
        memoryTotalBytes: container.memory.limitBytes,
      }

      const existing = map.get(container.containerId)

      if (existing) {
        existing.containerName = container.containerName
        existing.points = keepTailPoints([...existing.points, point])
      } else {
        map.set(container.containerId, {
          containerId: container.containerId,
          containerName: container.containerName,
          points: keepTailPoints([point]),
        })
      }
    }

    containerHistory.value = Array.from(map.values())
  }

  return {
    systemHistory,
    containerHistoryById,
    retentionDays,
    loading,
    error,
    load,
    appendSystemEvent,
    appendContainerOverview,
  }
}

function keepTailPoints(points: ResourceHistoryPoint[]): ResourceHistoryPoint[] {
  if (points.length <= MAX_VISIBLE_POINTS) {
    return points
  }

  return points.slice(points.length - MAX_VISIBLE_POINTS)
}
