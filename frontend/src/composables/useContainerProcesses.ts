import { computed, onMounted, onUnmounted, ref, toValue, watch, type MaybeRefOrGetter } from 'vue'
import type { DockerContainerTop, DockerContainerTopResources } from '@/types/api'
import { dockerContainersApi } from '@/services/dockerService'
import { formatBytes } from '@/utils/format'

export interface UseContainerProcessesOptions {
  defaultPsArgs?: string
  defaultInterval?: number
}

export function useContainerProcesses(
  containerId: MaybeRefOrGetter<string>,
  isRunning?: MaybeRefOrGetter<boolean | undefined>,
  options: UseContainerProcessesOptions = {}
) {
  const titles = ref<string[]>([])
  const processes = ref<string[][]>([])
  const resources = ref<DockerContainerTopResources | null>(null)
  const loading = ref(false)
  const isSilentUpdating = ref(false)
  const requestError = ref<string | null>(null)
  const lastUpdated = ref('')

  const psArgs = ref(options.defaultPsArgs ?? 'aux')
  const searchQuery = ref('')
  const sortColumnIndex = ref<number | null>(null)
  const sortDesc = ref(false)
  const autoRefreshInterval = ref(options.defaultInterval ?? 5000)

  let timer: ReturnType<typeof setInterval> | null = null

  const pidColIndex = computed(() => {
    return titles.value.findIndex((t) => t.toUpperCase() === 'PID')
  })

  const processCount = computed(() => {
    if (resources.value?.pids !== undefined && resources.value.pids > 0) {
      return resources.value.pids
    }
    return processes.value.length
  })

  const filteredProcesses = computed(() => {
    let list = processes.value

    const q = searchQuery.value.trim().toLowerCase()
    if (q) {
      list = list.filter((row) =>
        row.some((col) => typeof col === 'string' && col.toLowerCase().includes(q))
      )
    }

    if (sortColumnIndex.value !== null) {
      const colIdx = sortColumnIndex.value
      const desc = sortDesc.value
      list = [...list].sort((a, b) => {
        const valA = a[colIdx] ?? ''
        const valB = b[colIdx] ?? ''
        const numA = Number(valA)
        const numB = Number(valB)

        if (!isNaN(numA) && !isNaN(numB)) {
          return desc ? numB - numA : numA - numB
        }

        return desc ? valB.localeCompare(valA) : valA.localeCompare(valB)
      })
    }

    return list
  })

  function handleSort(colIdx: number): void {
    if (sortColumnIndex.value === colIdx) {
      if (sortDesc.value) {
        sortColumnIndex.value = null
        sortDesc.value = false
      } else {
        sortDesc.value = true
      }
    } else {
      sortColumnIndex.value = colIdx
      sortDesc.value = false
    }
  }

  async function load(silent = false): Promise<void> {
    const running = toValue(isRunning)
    const id = toValue(containerId)

    if (!id || (running !== undefined && !running)) {
      titles.value = []
      processes.value = []
      resources.value = null
      return
    }

    if (!silent) {
      loading.value = true
    } else {
      isSilentUpdating.value = true
    }
    requestError.value = null

    try {
      const data: DockerContainerTop = await dockerContainersApi.getTop(id, psArgs.value)

      // Only update titles if column headers changed to prevent <thead> re-rendering
      const newTitles = data.titles ?? []
      if (
        titles.value.length !== newTitles.length ||
        titles.value.some((t, i) => t !== newTitles[i])
      ) {
        titles.value = newTitles
      }

      processes.value = data.processes ?? []
      if (data.resources) {
        resources.value = data.resources
      }
      lastUpdated.value = new Date().toLocaleTimeString('pt-BR')
    } catch (err) {
      if (!silent) {
        requestError.value =
          err instanceof Error ? err.message : 'Falha ao carregar processos do container.'
      }
    } finally {
      if (!silent) {
        loading.value = false
      } else {
        isSilentUpdating.value = false
      }
    }
  }

  function setupAutoRefresh(): void {
    if (timer) {
      clearInterval(timer)
      timer = null
    }

    const running = toValue(isRunning)
    if (autoRefreshInterval.value > 0 && (running === undefined || running)) {
      timer = setInterval(() => {
        if (!loading.value && !isSilentUpdating.value) {
          void load(true)
        }
      }, autoRefreshInterval.value)
    }
  }

  async function copyDiagnosticText(): Promise<string | null> {
    if (processes.value.length === 0) return null

    const id = toValue(containerId)
    const header = `=== DIAGNÓSTICO DO CONTAINER: ${id.slice(0, 12)} ===\n`
    const date = `Data: ${new Date().toLocaleString('pt-BR')}\n`
    const cpu = resources.value ? `CPU: ${resources.value.cpuPercent.toFixed(1)}%\n` : ''
    const mem = resources.value
      ? `Memória: ${formatBytes(resources.value.memoryUsageBytes)} / ${formatBytes(resources.value.memoryLimitBytes)} (${resources.value.memoryUsagePercent.toFixed(1)}%)\n`
      : ''
    const pids = `Total de Processos: ${processes.value.length}\n\n`

    const tableHeader = titles.value.join('\t') + '\n'
    const tableRows = processes.value.map((row) => row.join('\t')).join('\n')

    const content = `${header}${date}${cpu}${mem}${pids}${tableHeader}${tableRows}\n`

    try {
      await navigator.clipboard.writeText(content)
      return content
    } catch {
      return null
    }
  }

  watch(autoRefreshInterval, setupAutoRefresh)

  if (isRunning !== undefined) {
    watch(
      () => toValue(isRunning),
      (running) => {
        if (running) {
          void load(false)
          setupAutoRefresh()
        } else {
          if (timer) {
            clearInterval(timer)
            timer = null
          }
          titles.value = []
          processes.value = []
          resources.value = null
        }
      }
    )
  }

  onMounted(() => {
    void load(false)
    setupAutoRefresh()
  })

  onUnmounted(() => {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  })

  return {
    titles,
    processes,
    resources,
    loading,
    isSilentUpdating,
    requestError,
    lastUpdated,
    psArgs,
    searchQuery,
    sortColumnIndex,
    sortDesc,
    autoRefreshInterval,
    pidColIndex,
    processCount,
    filteredProcesses,
    handleSort,
    load,
    copyDiagnosticText,
  }
}
