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
/** Interval used for live SSE appends when the selected range is ≤ 1h. */
const LIVE_UPDATE_INTERVAL_MS = 1_000

export function useResourceHistory(): UseResourceHistoryResult {
  const systemHistory = ref<ResourceHistoryPoint[]>([])
  const containerHistory = ref<ContainerResourceHistory[]>([])
  const retentionDays = ref(15)
  const loading = ref(false)
  const error = ref<string | null>(null)
  const currentRangeHours = ref(24)

  const containerHistoryById = computed<ContainerHistoryById>(() => {
    return containerHistory.value.reduce<ContainerHistoryById>((acc, item) => {
      acc[item.containerId] = item
      return acc
    }, {})
  })

  async function load(rangeHours = 24): Promise<void> {
    loading.value = true
    currentRangeHours.value = rangeHours

    try {
      const response = await systemApi.resourcesHistory(rangeHours)
      const data = response.data ?? ({} as ResourceMetricsHistoryResponse)

      retentionDays.value = data.retentionDays ?? 15
      systemHistory.value = downsamplePoints(data.system ?? [])
      containerHistory.value = (data.containers ?? []).map((container) => ({
        ...container,
        points: downsamplePoints(container.points ?? []),
      }))
      error.value = null
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Erro ao carregar histórico de recursos'
    } finally {
      loading.value = false
    }
  }

  /**
   * Minimum milliseconds that must elapse between consecutive data points.
   *
   * For short ranges (≤1h) a fixed 2 s cap is used so live SSE events
   * (~1s cadence) produce visible chart movement at the leading edge.
   * For longer ranges the natural spread (range / MAX_VISIBLE_POINTS) is
   * used to prevent the chart from scrolling too fast.
   */
  function minIntervalMs(): number {
    const natural = (currentRangeHours.value * 60 * 60 * 1000) / MAX_VISIBLE_POINTS
    return currentRangeHours.value <= 1 ? Math.min(natural, LIVE_UPDATE_INTERVAL_MS) : natural
  }

  /**
   * Returns true only when the event timestamp is far enough from the last
   * recorded point to warrant adding a new entry to the chart.
   */
  function isTimeForNewPoint(points: ResourceHistoryPoint[], eventTimestampIso: string): boolean {
    if (!points.length) return true
    const last = points[points.length - 1]
    if (!last) return true
    const elapsed = new Date(eventTimestampIso).getTime() - new Date(last.timestamp).getTime()
    return elapsed >= minIntervalMs()
  }

  function appendSystemEvent(event: SystemResourcesEvent): void {
    if (!isTimeForNewPoint(systemHistory.value, event.timestamp)) return

    systemHistory.value = downsamplePoints([
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
      const existing = map.get(container.containerId)

      if (existing && !isTimeForNewPoint(existing.points, collectedAt)) {
        continue
      }

      const point: ResourceHistoryPoint = {
        timestamp: collectedAt,
        cpuUsagePercent: container.cpu.usagePercent,
        memoryUsagePercent: container.memory.usagePercent,
        memoryUsedBytes: container.memory.usageBytes,
        memoryTotalBytes: container.memory.limitBytes,
      }

      if (existing) {
        existing.containerName = container.containerName
        existing.points = downsamplePoints([...existing.points, point])
      } else {
        map.set(container.containerId, {
          containerId: container.containerId,
          containerName: container.containerName,
          points: [point],
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

/**
 * Evenly distributes at most MAX_VISIBLE_POINTS points across the full time
 * span of the dataset, always preserving the first and last entry.
 * This ensures the chart's X-axis correctly represents the requested range
 * regardless of how many raw records the API returned.
 */
function downsamplePoints(points: ResourceHistoryPoint[]): ResourceHistoryPoint[] {
  if (points.length <= MAX_VISIBLE_POINTS) return points

  const step = (points.length - 1) / (MAX_VISIBLE_POINTS - 1)
  return Array.from({ length: MAX_VISIBLE_POINTS }, (_, i) => {
    const index = Math.min(Math.round(i * step), points.length - 1)
    return points[index]!
  })
}
