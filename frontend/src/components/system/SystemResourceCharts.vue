<template>
  <v-row v-if="system" class="mb-6">
    <v-col cols="12">
      <div class="d-flex align-center justify-space-between mb-4 flex-wrap ga-3">
        <div class="d-flex align-center">
          <v-icon class="mr-2" color="primary" icon="mdi-chart-donut" />
          <h2 class="text-h6 font-weight-medium">Recursos do Servidor</h2>
        </div>

        <v-btn-toggle
          v-if="hasContainerLimit"
          v-model="viewScope"
          mandatory
          density="compact"
          color="primary"
          variant="outlined"
          rounded="lg"
        >
          <v-btn value="host" size="small">
            <v-icon start icon="mdi-server" />
            Host (Servidor)
          </v-btn>
          <v-btn value="container" size="small">
            <v-icon start icon="mdi-docker" />
            Container
          </v-btn>
        </v-btn-toggle>
      </div>
    </v-col>

    <v-col
      v-for="metric in metricCards"
      :key="metric.key"
      cols="12"
      md="6"
    >
      <v-card class="resource-card">
        <v-card-text class="pa-5">
          <div class="d-flex align-center justify-space-between ga-4 flex-wrap">
            <div class="d-flex align-center ga-4">
              <div>
                <div class="d-flex align-center ga-2 mb-1">
                  <v-icon :color="metric.color" :icon="metric.icon" size="20" />
                  <h3 class="text-h6 font-weight-medium">{{ metric.title }}</h3>
                </div>
                <p class="text-body-2 text-medium-emphasis mb-0">
                  {{ metric.subtitle }}
                </p>
              </div>
            </div>

            <v-chip :color="metric.color" label size="small" variant="tonal">
              {{ metric.badge }}
            </v-chip>
          </div>

          <div class="mt-4">
            <div class="d-flex align-center justify-space-between mb-2">
              <span class="text-caption text-medium-emphasis">Histórico</span>
              <strong class="tabular-num">{{ metric.percentage.toFixed(1) }}%</strong>
            </div>
            <UsageLineChart
              :values="metric.historyValues"
              :timestamps="historyTimestamps"
              :range-hours="rangeHours"
              :color="resolveChartColor(metric.color)"
              :height="96"
              :raw-values="metric.historyRawValues"
              :raw-formatter="metric.rawFormatter"
            />
          </div>

          <v-divider class="my-4" />

          <div class="resource-grid">
            <div class="resource-metric">
              <span class="text-caption text-medium-emphasis">Em uso</span>
              <strong>{{ metric.primaryValue }}</strong>
            </div>

            <div class="resource-metric">
              <span class="text-caption text-medium-emphasis">{{ metric.secondaryLabel }}</span>
              <strong>{{ metric.secondaryValue }}</strong>
            </div>

            <div class="resource-metric">
              <span class="text-caption text-medium-emphasis">{{ metric.tertiaryLabel }}</span>
              <strong>{{ metric.tertiaryValue }}</strong>
            </div>
          </div>
        </v-card-text>
      </v-card>
    </v-col>
  </v-row>
</template>

<script lang="ts" setup>
import type { ResourceHistoryPoint, SystemStatus } from '@/types/api'
import { computed, ref } from 'vue'
import { formatBytes } from '@/utils/format'
import { resolveUsageColor, resolveChartColor } from '@/utils/dockerMetrics'
import UsageLineChart from './UsageLineChart.vue'

type MetricCard = {
  key: 'cpu' | 'memory'
  title: string
  subtitle: string
  icon: string
  color: string
  percentage: number
  badge: string
  historyValues: number[]
  historyRawValues?: number[]
  rawFormatter?: (v: number) => string
  primaryValue: string
  secondaryLabel: string
  secondaryValue: string
  tertiaryLabel: string
  tertiaryValue: string
}

const props = defineProps<{
  system: SystemStatus | null
  history: ResourceHistoryPoint[]
  rangeHours?: number
}>()

const viewScope = ref<'host' | 'container'>('host')

const hasContainerLimit = computed(() => {
  return Boolean(props.system?.resources.memory.containerLimited && props.system?.resources.memory.host)
})

const metricCards = computed<MetricCard[]>(() => {
  if (!props.system) return []

  const isHost = viewScope.value === 'host' && hasContainerLimit.value
  const hostMemory = props.system.resources.memory.host

  // CPU
  const cpuModel = props.system.resources.cpu.model
  const cpuUsage = props.system.resources.cpu.usagePercent
  const cpuCores = isHost && props.system.resources.cpu.hostCores
    ? props.system.resources.cpu.hostCores
    : props.system.resources.cpu.cores

  const cpuCard: MetricCard = {
    key: 'cpu',
    title: isHost ? 'CPU (Host)' : (hasContainerLimit.value ? 'CPU (Container)' : 'CPU'),
    subtitle: cpuModel,
    icon: 'mdi-cpu-64-bit',
    color: resolveUsageColor(cpuUsage),
    percentage: cpuUsage,
    badge: `${cpuCores} núcleo(s)`,
    historyValues: resolveHistoryValues('cpu'),
    primaryValue: `${cpuUsage.toFixed(1)}%`,
    secondaryLabel: 'Média Geral',
    secondaryValue: '100% total',
    tertiaryLabel: 'Arquitetura',
    tertiaryValue: props.system.architecture.toUpperCase(),
  }

  // RAM
  const ramTotalBytes = isHost && hostMemory
    ? hostMemory.totalBytes
    : props.system.resources.memory.totalBytes
  const ramUsedBytes = isHost && hostMemory
    ? hostMemory.usedBytes
    : props.system.resources.memory.usedBytes
  const ramFreeBytes = isHost && hostMemory
    ? hostMemory.freeBytes
    : props.system.resources.memory.freeBytes
  const ramUsagePercent = isHost && hostMemory
    ? hostMemory.usagePercent
    : props.system.resources.memory.usagePercent

  const ramSubtitle = isHost && hostMemory
    ? `Memória do servidor (Container: ${formatBytes(props.system.resources.memory.usedBytes)} / ${formatBytes(props.system.resources.memory.totalBytes)})`
    : (hasContainerLimit.value && hostMemory
      ? `Limite do container (Host: ${formatBytes(hostMemory.totalBytes)} total)`
      : 'Memória disponível para o servidor')

  const ramCard: MetricCard = {
    key: 'memory',
    title: isHost ? 'RAM (Host)' : (hasContainerLimit.value ? 'RAM (Container)' : 'RAM'),
    subtitle: ramSubtitle,
    icon: 'mdi-memory',
    color: resolveUsageColor(ramUsagePercent),
    percentage: ramUsagePercent,
    badge: `${formatBytes(ramFreeBytes)} livre`,
    historyValues: resolveHistoryValues('memory'),
    historyRawValues: resolveRawHistoryValues(),
    rawFormatter: formatBytes,
    primaryValue: formatBytes(ramUsedBytes),
    secondaryLabel: isHost ? 'Total' : (hasContainerLimit.value ? 'Limite' : 'Total'),
    secondaryValue: formatBytes(ramTotalBytes),
    tertiaryLabel: 'Livre',
    tertiaryValue: formatBytes(ramFreeBytes),
  }

  return [cpuCard, ramCard]
})

const historyTimestamps = computed(() => props.history.map((p) => p.timestamp))

function resolveHistoryValues(metric: 'cpu' | 'memory'): number[] {
  const isHost = viewScope.value === 'host' && hasContainerLimit.value
  const currentUsage = metric === 'cpu'
    ? (props.system?.resources.cpu.usagePercent ?? 0)
    : (isHost && props.system?.resources.memory.host
      ? props.system.resources.memory.host.usagePercent
      : (props.system?.resources.memory.usagePercent ?? 0))

  const values = props.history.map((point) =>
    metric === 'cpu' ? point.cpuUsagePercent : point.memoryUsagePercent
  )

  if (values.length >= 2) {
    return values
  }

  return [0, currentUsage]
}

function resolveRawHistoryValues(): number[] {
  const isHost = viewScope.value === 'host' && hasContainerLimit.value
  const currentUsed = isHost && props.system?.resources.memory.host
    ? props.system.resources.memory.host.usedBytes
    : (props.system?.resources.memory.usedBytes ?? 0)

  const values = props.history.map((point) => point.memoryUsedBytes)
  if (values.length >= 2) return values
  return [0, currentUsed]
}
</script>

<style scoped>
.resource-card {
  background: linear-gradient(135deg,
      rgb(var(--v-theme-surface)) 0%,
      rgb(var(--v-theme-surface-bright)) 100%);
  border: 1px solid rgba(var(--v-border-color), 0.08);
}

.resource-grid {
  display: grid;
  gap: 12px;
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.resource-metric {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.resource-metric strong,
.tabular-num {
  font-variant-numeric: tabular-nums;
}
</style>
