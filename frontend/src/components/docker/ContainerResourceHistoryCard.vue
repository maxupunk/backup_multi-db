<template>
  <v-card class="resource-history-card mb-4" variant="outlined">
    <v-card-text class="pa-4">
      <div class="d-flex align-center justify-space-between flex-wrap ga-3 mb-3">
        <div class="d-flex align-center ga-2">
          <v-avatar color="primary" rounded="lg" size="36" variant="tonal">
            <v-icon icon="mdi-chart-timeline-variant" size="20" />
          </v-avatar>
          <div>
            <h3 class="text-subtitle-1 font-weight-bold mb-0">
              Histórico de Recursos (CPU e Memória)
            </h3>
            <p class="text-caption text-medium-emphasis mb-0">
              Métricas consolidadas com atualização contínua em tempo real
            </p>
          </div>
        </div>

        <div class="d-flex align-center ga-2">
          <!-- Range Selector Toggle -->
          <v-btn-toggle
            v-model="selectedRangeHours"
            color="primary"
            density="compact"
            mandatory
            variant="outlined"
          >
            <v-btn
              v-for="opt in RANGE_OPTIONS"
              :key="opt.hours"
              size="small"
              :value="opt.hours"
            >
              {{ opt.label }}
            </v-btn>
          </v-btn-toggle>

          <v-btn
            density="compact"
            icon="mdi-refresh"
            :loading="resourceHistory.loading.value"
            title="Atualizar histórico"
            variant="tonal"
            @click="reloadHistory"
          />
        </div>
      </div>

      <v-divider class="mb-4" />

      <!-- Loading / Progress -->
      <v-progress-linear
        v-if="resourceHistory.loading.value && historyPoints.length === 0"
        color="primary"
        indeterminate
      />

      <!-- Container Stopped Alert -->
      <v-alert
        v-else-if="!isRunning && isRunning !== undefined"
        density="compact"
        icon="mdi-information-outline"
        type="info"
        variant="tonal"
      >
        O container não está em execução. Inicie-o para acompanhar as métricas em tempo real.
      </v-alert>

      <!-- Charts Grid -->
      <div v-else class="charts-wrapper">
        <v-row dense>
          <!-- CPU Chart -->
          <v-col cols="12" md="6">
            <v-card class="chart-box pa-3" variant="tonal">
              <div class="d-flex align-center justify-space-between mb-2">
                <div class="d-flex align-center ga-2">
                  <v-icon color="primary" icon="mdi-cpu-64-bit" size="18" />
                  <span class="text-caption font-weight-bold text-medium-emphasis">
                    CPU (1 núcleo = 100%)
                  </span>
                </div>
                <div class="d-flex align-center ga-2">
                  <span class="text-caption text-medium-emphasis">
                    Pico: <strong>{{ peakCpuPercent.toFixed(1) }}%</strong>
                  </span>
                  <v-chip
                    :color="resolveUsageColor(currentCpuPercent)"
                    label
                    size="x-small"
                    variant="tonal"
                  >
                    Atual: {{ currentCpuPercent.toFixed(1) }}%
                  </v-chip>
                </div>
              </div>

              <UsageLineChart
                :color="resolveChartColor(resolveUsageColor(currentCpuPercent))"
                :height="100"
                :range-hours="selectedRangeHours"
                :timestamps="timestamps"
                :values="cpuHistoryValues"
              />
            </v-card>
          </v-col>

          <!-- Memory Chart -->
          <v-col cols="12" md="6">
            <v-card class="chart-box pa-3" variant="tonal">
              <div class="d-flex align-center justify-space-between mb-2">
                <div class="d-flex align-center ga-2">
                  <v-icon color="primary" icon="mdi-memory" size="18" />
                  <span class="text-caption font-weight-bold text-medium-emphasis">
                    Memória RAM
                  </span>
                </div>
                <div class="d-flex align-center ga-2">
                  <span class="text-caption text-medium-emphasis">
                    Pico: <strong>{{ formatBytes(peakMemoryUsed) }}</strong>
                  </span>
                  <v-chip
                    :color="resolveUsageColor(currentMemoryPercent)"
                    label
                    size="x-small"
                    variant="tonal"
                  >
                    Atual: {{ currentMemoryPercent.toFixed(1) }}%
                  </v-chip>
                </div>
              </div>

              <UsageLineChart
                :color="resolveChartColor(resolveUsageColor(currentMemoryPercent))"
                :height="100"
                :range-hours="selectedRangeHours"
                :raw-formatter="formatBytes"
                :raw-values="memoryRawValues"
                :timestamps="timestamps"
                :values="memoryHistoryValues"
              />

              <div class="d-flex align-center justify-space-between text-caption text-medium-emphasis mt-1">
                <span>
                  {{ formatBytes(currentMemoryUsed) }} utilizados de {{ formatBytes(currentMemoryLimit) }}
                </span>
              </div>
            </v-card>
          </v-col>
        </v-row>

        <!-- Detailed metrics (I/O, Network, PIDs) -->
        <div v-if="containerMetrics" class="resource-grid resource-grid--details mt-3">
          <div class="resource-metric">
            <span class="text-caption text-medium-emphasis">
              <v-icon class="mr-1" icon="mdi-swap-horizontal" size="14" />
              Rede RX/TX
            </span>
            <strong class="text-caption">
              {{ formatBytes(containerMetrics.network.rxBytes) }} / {{ formatBytes(containerMetrics.network.txBytes) }}
            </strong>
          </div>

          <div class="resource-metric">
            <span class="text-caption text-medium-emphasis">
              <v-icon class="mr-1" icon="mdi-harddisk" size="14" />
              Disco R/W (Block I/O)
            </span>
            <strong class="text-caption">
              {{ formatBytes(containerMetrics.blockIo.readBytes) }} / {{ formatBytes(containerMetrics.blockIo.writeBytes) }}
            </strong>
          </div>

          <div class="resource-metric">
            <span class="text-caption text-medium-emphasis">
              <v-icon class="mr-1" icon="mdi-cogs" size="14" />
              PIDs
            </span>
            <strong class="text-caption">
              {{ containerMetrics.pids ?? 'N/A' }}
            </strong>
          </div>
        </div>
      </div>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue'
import type {
  DockerContainerResourceMetrics,
  ResourceHistoryPoint,
} from '@/types/api'
import { useDockerContainerResources } from '@/composables/useDockerContainerResources'
import { useResourceHistory } from '@/composables/useResourceHistory'
import { formatBytes } from '@/utils/format'
import { resolveUsageColor, resolveChartColor } from '@/utils/dockerMetrics'
import UsageLineChart from '@/components/system/UsageLineChart.vue'

const RANGE_OPTIONS = [
  { label: '1h', hours: 1 },
  { label: '24h', hours: 24 },
  { label: '7d', hours: 24 * 7 },
  { label: '15d', hours: 24 * 15 },
]

const props = defineProps<{
  containerId: string
  containerName?: string
  isRunning?: boolean
}>()

const selectedRangeHours = ref(24)
const { overview: dockerOverview } = useDockerContainerResources()
const resourceHistory = useResourceHistory()

const containerMetrics = computed<DockerContainerResourceMetrics | null>(() => {
  if (!dockerOverview.value?.containers) return null
  const id = props.containerId
  return (
    dockerOverview.value.containers.find(
      (c) =>
        c.containerId === id ||
        c.containerId.startsWith(id) ||
        id.startsWith(c.containerId)
    ) ?? null
  )
})

const historyPoints = computed<ResourceHistoryPoint[]>(() => {
  const id = props.containerId
  const map = resourceHistory.containerHistoryById.value

  if (map[id]?.points) {
    return map[id].points
  }

  for (const [key, hist] of Object.entries(map)) {
    if (key === id || key.startsWith(id) || id.startsWith(key)) {
      return hist.points ?? []
    }
  }

  return []
})

const timestamps = computed(() => historyPoints.value.map((point) => point.timestamp))

const currentCpuPercent = computed(() => containerMetrics.value?.cpu.usagePercent ?? 0)
const currentMemoryPercent = computed(() => containerMetrics.value?.memory.usagePercent ?? 0)
const currentMemoryUsed = computed(() => containerMetrics.value?.memory.usageBytes ?? 0)
const currentMemoryLimit = computed(() => containerMetrics.value?.memory.limitBytes ?? 0)

const peakCpuPercent = computed(() => {
  if (historyPoints.value.length === 0) return currentCpuPercent.value
  return Math.max(...historyPoints.value.map((p) => p.cpuUsagePercent), currentCpuPercent.value)
})

const peakMemoryUsed = computed(() => {
  if (historyPoints.value.length === 0) return currentMemoryUsed.value
  return Math.max(
    ...historyPoints.value.map((p) => p.memoryUsedBytes ?? 0),
    currentMemoryUsed.value
  )
})

const cpuHistoryValues = computed(() => {
  const values = historyPoints.value.map((point) => point.cpuUsagePercent)
  if (values.length >= 2) return values
  return [0, currentCpuPercent.value]
})

const memoryHistoryValues = computed(() => {
  const values = historyPoints.value.map((point) => point.memoryUsagePercent)
  if (values.length >= 2) return values
  return [0, currentMemoryPercent.value]
})

const memoryRawValues = computed(() => {
  const values = historyPoints.value.map((point) => point.memoryUsedBytes ?? 0)
  if (values.length >= 2) return values
  return [0, currentMemoryUsed.value]
})

async function reloadHistory(): Promise<void> {
  await resourceHistory.load(selectedRangeHours.value)
}

watch(dockerOverview, (overview) => {
  if (overview) {
    resourceHistory.appendContainerOverview(overview)
  }
})

watch(selectedRangeHours, (hours) => {
  void resourceHistory.load(hours)
})

onMounted(() => {
  void resourceHistory.load(selectedRangeHours.value)
})
</script>

<style scoped>
.resource-history-card {
  border-radius: 8px;
  background: linear-gradient(
    135deg,
    rgb(var(--v-theme-surface)) 0%,
    rgb(var(--v-theme-surface-bright)) 100%
  );
  border: 1px solid rgba(var(--v-border-color), 0.12);
}

.chart-box {
  border-radius: 6px;
  background: rgba(var(--v-theme-surface), 0.6);
  border: 1px solid rgba(var(--v-border-color), 0.08);
}

.resource-grid {
  display: grid;
  gap: 12px;
}

.resource-grid--details {
  grid-template-columns: repeat(3, minmax(0, 1fr));
  padding: 10px 12px;
  border-radius: 6px;
  background: rgba(var(--v-theme-primary), 0.03);
  border: 1px solid rgba(var(--v-theme-primary), 0.06);
}

.resource-metric {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

@media (max-width: 960px) {
  .resource-grid--details {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 600px) {
  .resource-grid--details {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
