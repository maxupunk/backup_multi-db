<template>
  <v-row class="mb-6">
    <v-col cols="12">
      <div class="d-flex align-center mb-4">
        <v-icon class="mr-2" color="primary" icon="mdi-memory" />
        <h2 class="text-h6 font-weight-medium">Heap do Processo</h2>
      </div>

      <v-card class="heap-toolbar-card mb-4">
        <v-card-text class="pa-4 pa-md-5">
          <div class="d-flex flex-column flex-lg-row align-lg-center justify-space-between ga-4">
            <div>
              <p class="text-body-2 mb-1">
                Acompanhamento do endpoint <strong>/api/system/heap</strong> para avaliar RSS, heap V8 e sinais auxiliares do processo.
              </p>
              <p class="text-caption text-medium-emphasis mb-0">
                Histórico local salvo neste navegador por até {{ retentionLabel }} e atualizado a cada {{ pollIntervalLabel }} enquanto o dashboard estiver aberto.
              </p>
            </div>

            <div class="d-flex flex-wrap align-center ga-2">
              <v-chip color="primary" label size="small" variant="tonal">
                {{ filteredSnapshots.length }} amostra(s)
              </v-chip>

              <v-chip :color="rssTrend.color" label size="small" variant="tonal">
                <v-icon :icon="rssTrend.icon" size="16" start />
                {{ rssTrend.label }}
              </v-chip>

              <v-chip v-if="lastUpdatedLabel" color="info" label size="small" variant="tonal">
                {{ lastUpdatedLabel }}
              </v-chip>

              <v-btn-toggle
                v-model="selectedRangeHours"
                color="primary"
                density="comfortable"
                divided
                mandatory
              >
                <v-btn
                  v-for="option in rangeOptions"
                  :key="option.hours"
                  :value="option.hours"
                  variant="text"
                >
                  {{ option.label }}
                </v-btn>
              </v-btn-toggle>

              <v-btn
                color="primary"
                :loading="loading"
                prepend-icon="mdi-refresh"
                variant="tonal"
                @click="emit('refresh')"
              >
                Atualizar
              </v-btn>

              <v-btn
                color="secondary"
                :disabled="!snapshots.length && !current"
                prepend-icon="mdi-delete-sweep-outline"
                variant="text"
                @click="emit('clear-history')"
              >
                Limpar histórico
              </v-btn>
            </div>
          </div>
        </v-card-text>
      </v-card>

      <v-alert
        v-if="error"
        class="mb-4"
        density="comfortable"
        type="warning"
        variant="tonal"
      >
        {{ error }}
      </v-alert>

      <v-alert
        v-if="latestSnapshot && filteredSnapshots.length < 2"
        class="mb-4"
        density="comfortable"
        type="info"
        variant="tonal"
      >
        Ainda não há amostras suficientes para desenhar tendência. Deixe o dashboard aberto por mais alguns ciclos para comparar a curva.
      </v-alert>

      <v-alert
        v-else-if="!latestSnapshot && loading"
        class="mb-4"
        density="comfortable"
        type="info"
        variant="tonal"
      >
        Carregando snapshot de heap do processo...
      </v-alert>
    </v-col>

    <template v-if="latestSnapshot">
      <v-col cols="12" md="6">
        <v-card class="heap-card">
          <v-card-text class="pa-5">
            <div class="d-flex align-center justify-space-between ga-4 flex-wrap">
              <div>
                <div class="d-flex align-center ga-2 mb-1">
                  <v-icon :color="heapUsageColor" icon="mdi-graphql" size="20" />
                  <h3 class="text-h6 font-weight-medium">Heap V8</h3>
                </div>
                <p class="text-body-2 text-medium-emphasis mb-0">
                  Uso efetivo do heap gerenciado pelo runtime.
                </p>
              </div>

              <v-chip :color="heapUsageColor" label size="small" variant="tonal">
                {{ latestSnapshot.heapUsagePercent.toFixed(1) }}%
              </v-chip>
            </div>

            <div class="mt-4">
              <div class="d-flex align-center justify-space-between mb-2">
                <span class="text-caption text-medium-emphasis">Janela selecionada</span>
                <strong>{{ formatDelta(heapDeltaBytes) }}</strong>
              </div>

              <UsageLineChart
                :values="heapUsageValues"
                :timestamps="historyTimestamps"
                :range-hours="selectedRangeHours"
                :color="resolveChartColor(heapUsageColor)"
                :height="96"
                :raw-values="heapUsedRawValues"
                :raw-formatter="formatBytes"
              />
            </div>

            <v-divider class="my-4" />

            <div class="heap-grid">
              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Heap usado</span>
                <strong>{{ formatBytes(latestSnapshot.heapUsedBytes) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Heap total</span>
                <strong>{{ formatBytes(latestSnapshot.heapTotalBytes) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Folga no heap</span>
                <strong>{{ formatBytes(heapHeadroomBytes) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Pico da janela</span>
                <strong>{{ formatBytes(windowPeakHeapBytes) }}</strong>
              </div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="6">
        <v-card class="heap-card">
          <v-card-text class="pa-5">
            <div class="d-flex align-center justify-space-between ga-4 flex-wrap">
              <div>
                <div class="d-flex align-center ga-2 mb-1">
                  <v-icon :color="rssTrend.color" icon="mdi-chart-line" size="20" />
                  <h3 class="text-h6 font-weight-medium">RSS do Processo</h3>
                </div>
                <p class="text-body-2 text-medium-emphasis mb-0">
                  Memória residente total do processo, incluindo heap, buffers e overhead nativo.
                </p>
              </div>

              <v-chip :color="rssTrend.color" label size="small" variant="tonal">
                <v-icon :icon="rssTrend.icon" size="16" start />
                {{ rssTrend.description }}
              </v-chip>
            </div>

            <div class="mt-4">
              <div class="d-flex align-center justify-space-between mb-2">
                <span class="text-caption text-medium-emphasis">Escala relativa ao pico da janela</span>
                <strong>{{ formatDelta(rssDeltaBytes) }}</strong>
              </div>

              <UsageLineChart
                :values="rssRelativeValues"
                :timestamps="historyTimestamps"
                :range-hours="selectedRangeHours"
                :color="resolveChartColor(rssTrend.color)"
                :height="96"
                :raw-values="rssRawValues"
                :raw-formatter="formatBytes"
              />
            </div>

            <v-divider class="my-4" />

            <div class="heap-grid">
              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">RSS atual</span>
                <strong>{{ formatBytes(latestSnapshot.rssBytes) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Pico da janela</span>
                <strong>{{ formatBytes(windowPeakRssBytes) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Início da janela</span>
                <strong>{{ formatBytes(windowStartRssBytes) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Fim da janela</span>
                <strong>{{ formatBytes(windowEndRssBytes) }}</strong>
              </div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12">
        <v-card class="heap-metadata-card">
          <v-card-text class="pa-5">
            <div class="d-flex align-center justify-space-between flex-wrap ga-4 mb-4">
              <div>
                <h3 class="text-h6 font-weight-medium mb-1">Sinais auxiliares do processo</h3>
                <p class="text-body-2 text-medium-emphasis mb-0">
                  Estes números ajudam a separar crescimento de heap V8 de buffers externos e acúmulo de handles ativos.
                </p>
              </div>

              <v-chip color="primary" label size="small" variant="tonal">
                Uptime {{ formatDuration(latestSnapshot.uptimeSeconds) }}
              </v-chip>
            </div>

            <div class="heap-metadata-grid">
              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Memória externa</span>
                <strong>{{ formatBytes(latestSnapshot.externalBytes) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Array buffers</span>
                <strong>{{ formatBytes(latestSnapshot.arrayBuffersBytes) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Handles ativos</span>
                <strong>{{ latestSnapshot.activeHandles }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Requests ativos</span>
                <strong>{{ latestSnapshot.activeRequests }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Última atualização</span>
                <strong>{{ formatDateTimePtBR(latestSnapshot.timestamp) }}</strong>
              </div>

              <div class="heap-metric">
                <span class="text-caption text-medium-emphasis">Janela atual</span>
                <strong>{{ selectedRangeLabel }}</strong>
              </div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </template>
  </v-row>
</template>

<script lang="ts" setup>
import type { SystemHeapSnapshot } from '@/types/api'
import { computed, ref } from 'vue'
import { formatBytes, formatDateTimePtBR, formatDuration } from '@/utils/format'
import UsageLineChart from './UsageLineChart.vue'

type TrendState = {
  label: string
  description: string
  color: 'success' | 'warning' | 'info'
  icon: string
}

const WINDOW_STABLE_DELTA_BYTES = 5 * 1024 * 1024
const rangeOptionsBase = [
  { label: '1h', hours: 1 },
  { label: '6h', hours: 6 },
  { label: '24h', hours: 24 },
  { label: '48h', hours: 48 },
]

const props = defineProps<{
  current: SystemHeapSnapshot | null
  snapshots: SystemHeapSnapshot[]
  loading: boolean
  error: string | null
  pollIntervalMs: number
  retentionHours: number
}>()

const emit = defineEmits<{
  refresh: []
  'clear-history': []
}>()

const selectedRangeHours = ref(1)

const rangeOptions = computed(() =>
  rangeOptionsBase.filter(option => option.hours <= props.retentionHours)
)

const latestSnapshot = computed(() => props.current ?? props.snapshots[props.snapshots.length - 1] ?? null)

const filteredSnapshots = computed(() => {
  const latest = latestSnapshot.value
  if (!latest) {
    return []
  }

  const cutoff = Date.now() - selectedRangeHours.value * 60 * 60 * 1000
  const next = props.snapshots.filter((snapshot) => new Date(snapshot.timestamp).getTime() >= cutoff)

  if (next.length) {
    return next
  }

  return [latest]
})

const historyTimestamps = computed(() => filteredSnapshots.value.map(snapshot => snapshot.timestamp))
const heapUsageValues = computed(() => filteredSnapshots.value.map(snapshot => snapshot.heapUsagePercent))
const heapUsedRawValues = computed(() => filteredSnapshots.value.map(snapshot => snapshot.heapUsedBytes))
const rssRawValues = computed(() => filteredSnapshots.value.map(snapshot => snapshot.rssBytes))

const windowPeakRssBytes = computed(() =>
  Math.max(...filteredSnapshots.value.map(snapshot => snapshot.rssBytes), latestSnapshot.value?.rssBytes ?? 0)
)

const rssRelativeValues = computed(() => {
  const peak = Math.max(windowPeakRssBytes.value, 1)
  return filteredSnapshots.value.map(snapshot => (snapshot.rssBytes / peak) * 100)
})

const windowPeakHeapBytes = computed(() =>
  Math.max(...filteredSnapshots.value.map(snapshot => snapshot.heapUsedBytes), latestSnapshot.value?.heapUsedBytes ?? 0)
)

const windowStartSnapshot = computed(() => filteredSnapshots.value[0] ?? null)
const windowEndSnapshot = computed(() => filteredSnapshots.value[filteredSnapshots.value.length - 1] ?? null)

const rssDeltaBytes = computed(() => {
  if (!windowStartSnapshot.value || !windowEndSnapshot.value) {
    return 0
  }

  return windowEndSnapshot.value.rssBytes - windowStartSnapshot.value.rssBytes
})

const heapDeltaBytes = computed(() => {
  if (!windowStartSnapshot.value || !windowEndSnapshot.value) {
    return 0
  }

  return windowEndSnapshot.value.heapUsedBytes - windowStartSnapshot.value.heapUsedBytes
})

const heapHeadroomBytes = computed(() => {
  if (!latestSnapshot.value) {
    return 0
  }

  return Math.max(0, latestSnapshot.value.heapTotalBytes - latestSnapshot.value.heapUsedBytes)
})

const heapUsageColor = computed(() => resolveUsageColor(latestSnapshot.value?.heapUsagePercent ?? 0))

const rssTrend = computed<TrendState>(() => {
  if (filteredSnapshots.value.length < 2) {
    return {
      label: 'Baseline em formação',
      description: 'Aguardando curva',
      color: 'info',
      icon: 'mdi-timer-sand',
    }
  }

  const delta = rssDeltaBytes.value

  if (Math.abs(delta) < WINDOW_STABLE_DELTA_BYTES) {
    return {
      label: 'RSS estável',
      description: formatDelta(delta),
      color: 'success',
      icon: 'mdi-check-circle-outline',
    }
  }

  if (delta > 0) {
    return {
      label: 'RSS em alta',
      description: formatDelta(delta),
      color: 'warning',
      icon: 'mdi-trending-up',
    }
  }

  return {
    label: 'RSS em queda',
    description: formatDelta(delta),
    color: 'info',
    icon: 'mdi-trending-down',
  }
})

const lastUpdatedLabel = computed(() => {
  if (!latestSnapshot.value) {
    return ''
  }

  return `Atualizado ${formatDateTimePtBR(latestSnapshot.value.timestamp, { withYear: false })}`
})

const selectedRangeLabel = computed(() => `${selectedRangeHours.value}h`)
const retentionLabel = computed(() => `${props.retentionHours}h`)
const pollIntervalLabel = computed(() => `${Math.round(props.pollIntervalMs / 1000)}s`)
const windowStartRssBytes = computed(() => windowStartSnapshot.value?.rssBytes ?? 0)
const windowEndRssBytes = computed(() => windowEndSnapshot.value?.rssBytes ?? 0)

function resolveUsageColor (percentage: number): 'success' | 'warning' | 'error' {
  if (percentage >= 85) return 'error'
  if (percentage >= 65) return 'warning'
  return 'success'
}

function resolveChartColor (color: string): string {
  if (color === 'error') return 'rgb(var(--v-theme-error))'
  if (color === 'warning') return 'rgb(var(--v-theme-warning))'
  if (color === 'success') return 'rgb(var(--v-theme-success))'
  if (color === 'info') return 'rgb(var(--v-theme-info))'
  return 'rgb(var(--v-theme-primary))'
}

function formatDelta (value: number): string {
  if (value === 0) {
    return '0 B'
  }

  const prefix = value > 0 ? '+' : '-'
  return `${prefix}${formatBytes(Math.abs(value))}`
}
</script>

<style scoped>
.heap-toolbar-card,
.heap-card,
.heap-metadata-card {
  background: linear-gradient(135deg,
      rgb(var(--v-theme-surface)) 0%,
      rgb(var(--v-theme-surface-bright)) 100%);
  border: 1px solid rgba(var(--v-border-color), 0.08);
}

.heap-grid {
  display: grid;
  gap: 12px;
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.heap-metadata-grid {
  display: grid;
  gap: 12px;
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.heap-metric {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

@media (max-width: 960px) {
  .heap-metadata-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 600px) {
  .heap-grid,
  .heap-metadata-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>