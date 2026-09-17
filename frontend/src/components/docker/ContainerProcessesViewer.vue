<template>
  <div>
    <!-- Not Running Warning -->
    <v-alert
      v-if="!isRunning"
      class="mb-4"
      density="comfortable"
      icon="mdi-alert-circle-outline"
      type="warning"
      variant="tonal"
    >
      O container não está em execução. Inicie o container para visualizar os processos ativos e o diagnóstico de CPU e memória em tempo real.
    </v-alert>

    <!-- Error Alert -->
    <v-alert
      v-else-if="requestError"
      class="mb-4"
      closable
      density="comfortable"
      icon="mdi-alert"
      type="error"
      variant="tonal"
      @click:close="requestError = null"
    >
      {{ requestError }}
    </v-alert>

    <!-- Live Resource Diagnostic Cards -->
    <v-row v-if="isRunning && resources" class="mb-4" dense>
      <!-- CPU Card -->
      <v-col cols="12" md="4" sm="6">
        <v-card class="process-metric-card h-100" variant="outlined">
          <v-card-text class="d-flex align-center justify-space-between py-3">
            <div>
              <div class="text-caption text-medium-emphasis font-weight-medium mb-1">
                <v-icon class="mr-1" color="primary" icon="mdi-cpu-64-bit" size="small" />
                USO DE CPU
              </div>
              <div class="text-h6 font-weight-bold">
                {{ resources.cpuPercent.toFixed(1) }}%
              </div>
              <div class="text-caption text-medium-emphasis">
                {{ resources.cpuPercent > 100 ? `${(resources.cpuPercent / 100).toFixed(1)} núcleos` : 'Carga instantânea' }}
              </div>
            </div>
            <v-progress-circular
              :color="resolveUsageColor(resources.cpuPercent)"
              :model-value="Math.min(100, resources.cpuPercent)"
              :rotate="-90"
              :size="58"
              :width="6"
            >
              <span class="text-caption font-weight-bold">
                {{ resources.cpuPercent.toFixed(0) }}%
              </span>
            </v-progress-circular>
          </v-card-text>
        </v-card>
      </v-col>

      <!-- Memory Card -->
      <v-col cols="12" md="4" sm="6">
        <v-card class="process-metric-card h-100" variant="outlined">
          <v-card-text class="d-flex align-center justify-space-between py-3">
            <div>
              <div class="text-caption text-medium-emphasis font-weight-medium mb-1">
                <v-icon class="mr-1" color="primary" icon="mdi-memory" size="small" />
                USO DE MEMÓRIA
              </div>
              <div class="text-h6 font-weight-bold">
                {{ formatBytes(resources.memoryUsageBytes) }}
              </div>
              <div class="text-caption text-medium-emphasis">
                {{ resources.memoryUsagePercent.toFixed(1) }}% de {{ formatBytes(resources.memoryLimitBytes) }}
              </div>
            </div>
            <v-progress-circular
              :color="resolveUsageColor(resources.memoryUsagePercent)"
              :model-value="Math.min(100, resources.memoryUsagePercent)"
              :rotate="-90"
              :size="58"
              :width="6"
            >
              <span class="text-caption font-weight-bold">
                {{ resources.memoryUsagePercent.toFixed(0) }}%
              </span>
            </v-progress-circular>
          </v-card-text>
        </v-card>
      </v-col>

      <!-- Processes / PIDs Card -->
      <v-col cols="12" md="4" sm="12">
        <v-card class="process-metric-card h-100" variant="outlined">
          <v-card-text class="d-flex align-center justify-space-between py-3">
            <div>
              <div class="text-caption text-medium-emphasis font-weight-medium mb-1">
                <v-icon class="mr-1" color="primary" icon="mdi-cogs" size="small" />
                PROCESSOS ATIVOS
              </div>
              <div class="text-h6 font-weight-bold">
                {{ processCount }}
              </div>
              <div class="text-caption text-medium-emphasis">
                <v-chip
                  class="mt-1"
                  color="success"
                  density="compact"
                  label
                  size="x-small"
                  variant="tonal"
                >
                  Em execução
                </v-chip>
              </div>
            </div>
            <v-avatar color="primary" rounded="lg" size="48" variant="tonal">
              <v-icon icon="mdi-format-list-numbered" size="24" />
            </v-avatar>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <!-- Toolbar & Filters -->
    <div class="d-flex align-center flex-wrap ga-2 mb-3">
      <!-- Filter text -->
      <v-text-field
        v-model="searchQuery"
        clearable
        density="compact"
        hide-details
        placeholder="Filtrar por PID, usuário, comando..."
        prepend-inner-icon="mdi-magnify"
        style="max-width: 320px"
        variant="outlined"
      />

      <!-- PS Arguments preset -->
      <v-select
        v-model="psArgs"
        density="compact"
        hide-details
        :items="PS_OPTIONS"
        label="Argumentos ps"
        style="max-width: 140px"
        variant="outlined"
        @update:model-value="load"
      />

      <!-- Auto-refresh interval -->
      <v-select
        v-model="autoRefreshInterval"
        density="compact"
        hide-details
        :items="REFRESH_OPTIONS"
        label="Atualização automática"
        style="max-width: 170px"
        variant="outlined"
      />

      <v-spacer />

      <!-- Copy Diagnostic -->
      <v-btn
        density="compact"
        :disabled="!isRunning || processes.length === 0"
        prepend-icon="mdi-content-copy"
        variant="tonal"
        @click="copyDiagnosticText"
      >
        Copiar diagnóstico
      </v-btn>

      <!-- Manual Refresh -->
      <v-btn
        color="primary"
        density="compact"
        :disabled="!isRunning"
        :loading="loading"
        prepend-icon="mdi-refresh"
        variant="tonal"
        @click="load"
      >
        Atualizar
      </v-btn>
    </div>

    <!-- Process Table -->
    <v-card variant="outlined">
      <v-progress-linear v-if="loading" color="primary" indeterminate />

      <div class="table-container">
        <v-table density="compact" fixed-header hover style="max-height: 520px">
          <thead>
            <tr>
              <th
                v-for="(title, idx) in titles"
                :key="idx"
                class="text-no-wrap cursor-pointer user-select-none font-weight-bold"
                @click="handleSort(idx)"
              >
                {{ title }}
                <v-icon
                  v-if="sortColumnIndex === idx"
                  :icon="sortDesc ? 'mdi-arrow-down' : 'mdi-arrow-up'"
                  size="small"
                />
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, rIdx) in filteredProcesses" :key="rIdx">
              <td
                v-for="(cell, cIdx) in row"
                :key="cIdx"
                :class="resolveCellClass(cIdx)"
              >
                <!-- Highlight PID column -->
                <template v-if="isPidColumn(cIdx)">
                  <span class="font-weight-bold text-caption text-primary">{{ cell }}</span>
                </template>

                <!-- Command / CMD column -->
                <template v-else-if="isCommandColumn(cIdx)">
                  <code class="process-command text-caption">{{ cell }}</code>
                </template>

                <!-- CPU / MEM columns -->
                <template v-else-if="isMetricColumn(cIdx)">
                  <span :class="Number(cell) > 0 ? 'font-weight-bold text-high-emphasis' : 'text-medium-emphasis'">
                    {{ cell }}%
                  </span>
                </template>

                <!-- Default text cell -->
                <template v-else>
                  <span class="text-caption">{{ cell }}</span>
                </template>
              </td>
            </tr>

            <tr v-if="filteredProcesses.length === 0 && !loading">
              <td
                class="text-caption text-medium-emphasis text-center py-6"
                :colspan="Math.max(1, titles.length)"
              >
                {{ isRunning ? 'Nenhum processo encontrado com o filtro atual.' : 'Container inativo.' }}
              </td>
            </tr>
          </tbody>
        </v-table>
      </div>

      <v-divider />

      <div class="d-flex align-center justify-space-between px-4 py-2 text-caption text-medium-emphasis">
        <span>
          Total: <strong>{{ filteredProcesses.length }}</strong> de <strong>{{ processes.length }}</strong> processos
        </span>
        <span v-if="lastUpdated">
          Última atualização: {{ lastUpdated }}
        </span>
      </div>
    </v-card>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type { DockerContainerTop, DockerContainerTopResources } from '@/types/api'
import { dockerContainersApi } from '@/services/dockerService'
import { useNotifier } from '@/composables/useNotifier'
import { formatBytes } from '@/utils/format'

const PS_OPTIONS = [
  { title: 'aux (padrão)', value: 'aux' },
  { title: '-ef (completo)', value: '-ef' },
  { title: 'ax (simples)', value: 'ax' },
  { title: 'top', value: 'top' },
]

const REFRESH_OPTIONS = [
  { title: 'Manual', value: 0 },
  { title: 'A cada 3s', value: 3000 },
  { title: 'A cada 5s', value: 5000 },
  { title: 'A cada 10s', value: 10000 },
  { title: 'A cada 30s', value: 30000 },
]

const props = defineProps<{
  containerId: string
  isRunning?: boolean
}>()

const notify = useNotifier()
const loading = ref(false)
const requestError = ref<string | null>(null)
const titles = ref<string[]>([])
const processes = ref<string[][]>([])
const resources = ref<DockerContainerTopResources | null>(null)
const psArgs = ref('aux')
const searchQuery = ref('')
const sortColumnIndex = ref<number | null>(null)
const sortDesc = ref(false)
const autoRefreshInterval = ref(5000)
const lastUpdated = ref('')
let timer: ReturnType<typeof setInterval> | null = null

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
      row.some((col) => col.toLowerCase().includes(q))
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

      return desc
        ? valB.localeCompare(valA)
        : valA.localeCompare(valB)
    })
  }

  return list
})

function resolveUsageColor(percentage: number): string {
  if (percentage >= 85) return 'error'
  if (percentage >= 65) return 'warning'
  return 'success'
}

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

function isPidColumn(colIdx: number): boolean {
  const colName = titles.value[colIdx]?.toUpperCase() ?? ''
  return colName === 'PID'
}

function isCommandColumn(colIdx: number): boolean {
  const colName = titles.value[colIdx]?.toUpperCase() ?? ''
  return colName === 'COMMAND' || colName === 'CMD'
}

function isMetricColumn(colIdx: number): boolean {
  const colName = titles.value[colIdx]?.toUpperCase() ?? ''
  return colName === '%CPU' || colName === '%MEM'
}

function resolveCellClass(colIdx: number): string {
  if (isCommandColumn(colIdx)) {
    return 'command-cell'
  }
  return 'text-no-wrap'
}

async function load(): Promise<void> {
  if (!props.isRunning && props.isRunning !== undefined) {
    titles.value = []
    processes.value = []
    resources.value = null
    return
  }

  loading.value = true
  requestError.value = null

  try {
    const data: DockerContainerTop = await dockerContainersApi.getTop(
      props.containerId,
      psArgs.value
    )

    titles.value = data.titles ?? []
    processes.value = data.processes ?? []
    resources.value = data.resources ?? null
    lastUpdated.value = new Date().toLocaleTimeString('pt-BR')
  } catch (err) {
    requestError.value = err instanceof Error ? err.message : 'Falha ao carregar processos do container.'
  } finally {
    loading.value = false
  }
}

function setupAutoRefresh(): void {
  if (timer) {
    clearInterval(timer)
    timer = null
  }

  if (autoRefreshInterval.value > 0 && props.isRunning) {
    timer = setInterval(() => {
      if (!loading.value) {
        void load()
      }
    }, autoRefreshInterval.value)
  }
}

function copyDiagnosticText(): void {
  if (processes.value.length === 0) return

  const header = `=== DIAGNÓSTICO DO CONTAINER: ${props.containerId.slice(0, 12)} ===\n`
  const date = `Data: ${new Date().toLocaleString('pt-BR')}\n`
  const cpu = resources.value ? `CPU: ${resources.value.cpuPercent.toFixed(1)}%\n` : ''
  const mem = resources.value
    ? `Memória: ${formatBytes(resources.value.memoryUsageBytes)} / ${formatBytes(resources.value.memoryLimitBytes)} (${resources.value.memoryUsagePercent.toFixed(1)}%)\n`
    : ''
  const pids = `Total de Processos: ${processes.value.length}\n\n`

  const tableHeader = titles.value.join('\t') + '\n'
  const tableRows = processes.value.map((row) => row.join('\t')).join('\n')

  const content = `${header}${date}${cpu}${mem}${pids}${tableHeader}${tableRows}\n`

  navigator.clipboard.writeText(content).then(
    () => {
      notify('Diagnóstico e lista de processos copiados para a área de transferência!', 'success')
    },
    () => {
      notify('Não foi possível copiar para a área de transferência.', 'error')
    }
  )
}

watch(autoRefreshInterval, setupAutoRefresh)
watch(
  () => props.isRunning,
  (running) => {
    if (running) {
      void load()
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

onMounted(() => {
  void load()
  setupAutoRefresh()
})

onUnmounted(() => {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
})
</script>

<style scoped>
.process-metric-card {
  border-radius: 8px;
  background: linear-gradient(135deg,
      rgb(var(--v-theme-surface)) 0%,
      rgb(var(--v-theme-surface-bright)) 100%);
  border: 1px solid rgba(var(--v-border-color), 0.12);
}

.table-container {
  overflow-x: auto;
}

.process-command {
  font-family: 'JetBrains Mono', 'Fira Code', Menlo, Monaco, Consolas, 'Courier New', monospace;
  font-size: 0.75rem;
  background-color: rgba(var(--v-theme-on-surface), 0.05);
  padding: 2px 6px;
  border-radius: 4px;
  display: inline-block;
  max-width: 600px;
  overflow-wrap: break-word;
  white-space: pre-wrap;
  line-height: 1.3;
}

.command-cell {
  max-width: 620px;
}

.cursor-pointer {
  cursor: pointer;
}

.user-select-none {
  user-select: none;
}
</style>
