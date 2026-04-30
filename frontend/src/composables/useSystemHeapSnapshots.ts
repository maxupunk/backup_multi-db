import type { SystemHeapSnapshot } from '@/types/api'
import { onMounted, onUnmounted, ref, type Ref } from 'vue'
import { systemApi } from '@/services/api'

type UseSystemHeapSnapshotsOptions = {
  pollIntervalMs?: number
  retentionHours?: number
  storageKey?: string
}

type UseSystemHeapSnapshotsResult = {
  current: Ref<SystemHeapSnapshot | null>
  history: Ref<SystemHeapSnapshot[]>
  loading: Ref<boolean>
  error: Ref<string | null>
  pollIntervalMs: number
  retentionHours: number
  refresh: () => Promise<void>
  clearHistory: () => void
}

const DEFAULT_POLL_INTERVAL_MS = 10_000
const DEFAULT_RETENTION_HOURS = 48
const DEFAULT_STORAGE_KEY = 'backup-multi-db.system-heap-history.v1'
const MAX_STORED_SNAPSHOTS = 20_000

export function useSystemHeapSnapshots(
  options: UseSystemHeapSnapshotsOptions = {}
): UseSystemHeapSnapshotsResult {
  const pollIntervalMs = options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS
  const retentionHours = options.retentionHours ?? DEFAULT_RETENTION_HOURS
  const storageKey = options.storageKey ?? DEFAULT_STORAGE_KEY

  const current = ref<SystemHeapSnapshot | null>(null)
  const history = ref<SystemHeapSnapshot[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  let intervalHandle: ReturnType<typeof setInterval> | null = null

  async function refresh (): Promise<void> {
    if (loading.value) {
      return
    }

    loading.value = true

    try {
      const response = await systemApi.heap()
      const snapshot = response.data ?? null

      if (!snapshot) {
        error.value = 'Resposta vazia ao consultar heap do processo'
        return
      }

      const nextHistory = pruneSnapshots(
        [...history.value.filter(item => item.timestamp !== snapshot.timestamp), snapshot],
        retentionHours,
      )

      history.value = nextHistory
      current.value = snapshot
      persistHistory(storageKey, nextHistory)
      error.value = null
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Erro ao carregar heap do processo'
    } finally {
      loading.value = false
    }
  }

  function clearHistory (): void {
    history.value = current.value ? [current.value] : []
    persistHistory(storageKey, history.value)
  }

  onMounted(async () => {
    history.value = restoreHistory(storageKey, retentionHours)
    current.value = history.value[history.value.length - 1] ?? null

    await refresh()

    intervalHandle = setInterval(() => {
      void refresh()
    }, pollIntervalMs)
  })

  onUnmounted(() => {
    if (intervalHandle) {
      clearInterval(intervalHandle)
      intervalHandle = null
    }
  })

  return {
    current,
    history,
    loading,
    error,
    pollIntervalMs,
    retentionHours,
    refresh,
    clearHistory,
  }
}

function canUseLocalStorage (): boolean {
  return typeof window !== 'undefined' && typeof window.localStorage !== 'undefined'
}

function restoreHistory (storageKey: string, retentionHours: number): SystemHeapSnapshot[] {
  if (!canUseLocalStorage()) {
    return []
  }

  try {
    const raw = window.localStorage.getItem(storageKey)
    if (!raw) {
      return []
    }

    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) {
      return []
    }

    return pruneSnapshots(parsed.filter(isSystemHeapSnapshot), retentionHours)
  } catch {
    return []
  }
}

function persistHistory (storageKey: string, snapshots: SystemHeapSnapshot[]): void {
  if (!canUseLocalStorage()) {
    return
  }

  try {
    window.localStorage.setItem(storageKey, JSON.stringify(snapshots))
  } catch {
    // noop
  }
}

function pruneSnapshots (
  snapshots: SystemHeapSnapshot[],
  retentionHours: number,
): SystemHeapSnapshot[] {
  const cutoff = Date.now() - retentionHours * 60 * 60 * 1000

  return [...snapshots]
    .filter((snapshot) => {
      const timestamp = new Date(snapshot.timestamp).getTime()
      return Number.isFinite(timestamp) && timestamp >= cutoff
    })
    .sort((left, right) => new Date(left.timestamp).getTime() - new Date(right.timestamp).getTime())
    .slice(-MAX_STORED_SNAPSHOTS)
}

function isSystemHeapSnapshot (value: unknown): value is SystemHeapSnapshot {
  if (!value || typeof value !== 'object') {
    return false
  }

  const snapshot = value as Record<string, unknown>
  const numericFields: Array<keyof SystemHeapSnapshot> = [
    'rssBytes',
    'heapTotalBytes',
    'heapUsedBytes',
    'heapUsagePercent',
    'externalBytes',
    'arrayBuffersBytes',
    'activeHandles',
    'activeRequests',
    'uptimeSeconds',
  ]

  return typeof snapshot.timestamp === 'string'
    && numericFields.every((field) => typeof snapshot[field] === 'number' && Number.isFinite(snapshot[field]))
}