<template>
  <v-row v-if="system" class="mb-6">
    <v-col cols="12">
      <div class="d-flex align-center mb-4">
        <v-icon class="mr-2" color="primary" icon="mdi-chart-donut" />
        <h2 class="text-h6 font-weight-medium">Recursos do Servidor</h2>
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
              <strong>{{ metric.percentage.toFixed(1) }}%</strong>
            </div>
            <UsageLineChart
              :values="metric.historyValues"
              :timestamps="historyTimestamps"
              :range-hours="rangeHours"
              :color="resolveChartColor(metric.color)"
              :height="96"
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
import { computed } from 'vue'
import { formatBytes } from '@/utils/format'
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

const metricCards = computed<MetricCard[]>(() => {
  if (!props.system) return []

  return [
    {
      key: 'cpu',
      title: 'CPU',
      subtitle: props.system.resources.cpu.model,
      icon: 'mdi-cpu-64-bit',
      color: resolveUsageColor(props.system.resources.cpu.usagePercent),
      percentage: props.system.resources.cpu.usagePercent,
      badge: `${props.system.resources.cpu.cores} núcleo(s)`,
      historyValues: resolveHistoryValues('cpu'),
      primaryValue: `${props.system.resources.cpu.usagePercent.toFixed(1)}%`,
      secondaryLabel: 'Capacidade',
      secondaryValue: '100%',
      tertiaryLabel: 'Arquitetura',
      tertiaryValue: props.system.architecture.toUpperCase(),
    },
    {
      key: 'memory',
      title: 'RAM',
      subtitle: 'Memória disponível para o servidor',
      icon: 'mdi-memory',
      color: resolveUsageColor(props.system.resources.memory.usagePercent),
      percentage: props.system.resources.memory.usagePercent,
      badge: `${formatBytes(props.system.resources.memory.freeBytes)} livre`,
      historyValues: resolveHistoryValues('memory'),
      primaryValue: formatBytes(props.system.resources.memory.usedBytes),
      secondaryLabel: 'Total',
      secondaryValue: formatBytes(props.system.resources.memory.totalBytes),
      tertiaryLabel: 'Livre',
      tertiaryValue: formatBytes(props.system.resources.memory.freeBytes),
    },
  ]
})

function resolveUsageColor(percentage: number): string {
  if (percentage >= 85) return 'error'
  if (percentage >= 65) return 'warning'
  return 'success'
}

const historyTimestamps = computed(() => props.history.map((p) => p.timestamp))

function resolveHistoryValues(metric: 'cpu' | 'memory'): number[] {
  const values = props.history.map((point) =>
    metric === 'cpu' ? point.cpuUsagePercent : point.memoryUsagePercent
  )

  if (values.length >= 2) {
    return values
  }

  return [0, metric === 'cpu' ? props.system?.resources.cpu.usagePercent ?? 0 : props.system?.resources.memory.usagePercent ?? 0]
}

function resolveChartColor(color: string): string {
  if (color === 'error') return 'rgb(var(--v-theme-error))'
  if (color === 'warning') return 'rgb(var(--v-theme-warning))'
  if (color === 'success') return 'rgb(var(--v-theme-success))'
  return 'rgb(var(--v-theme-primary))'
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
</style>
