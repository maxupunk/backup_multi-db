<template>
  <v-card class="resource-card">
    <v-card-text class="pa-5">
      <div class="d-flex align-center justify-space-between ga-3 mb-4">
        <div>
          <h3 class="text-subtitle-1 font-weight-bold mb-1">
            {{ container.containerName }}
          </h3>
          <p class="text-caption text-medium-emphasis mb-0">
            {{ container.imageName }}
          </p>
        </div>

        <div class="d-flex align-center ga-2">
          <v-chip :color="statusColor" label size="small" variant="tonal">
            {{ container.status }}
          </v-chip>

          <v-btn
            class="resource-toggle-btn"
            density="comfortable"
            variant="text"
            @click="toggleExpanded"
          >
            <v-icon icon="mdi-information-outline" start />
            <v-icon :icon="expanded ? 'mdi-chevron-up' : 'mdi-chevron-down'" />
          </v-btn>
        </div>
      </div>

      <div class="resource-summary">
        <div class="summary-metric">
          <v-progress-circular
            :color="resolveUsageColor(container.cpu.usagePercent)"
            :model-value="container.cpu.usagePercent"
            :rotate="-90"
            :size="76"
            :width="8"
            reveal
          >
            <span class="text-caption font-weight-bold">{{ container.cpu.usagePercent.toFixed(0) }}%</span>
          </v-progress-circular>

          <div class="summary-copy">
            <span class="text-caption text-medium-emphasis d-block">CPU</span>
            <strong>{{ container.cpu.usagePercent.toFixed(1) }}%</strong>
          </div>
        </div>

        <div class="summary-metric">
          <v-progress-circular
            :color="resolveUsageColor(container.memory.usagePercent)"
            :model-value="container.memory.usagePercent"
            :rotate="-90"
            :size="76"
            :width="8"
            reveal
          >
            <span class="text-caption font-weight-bold">{{ container.memory.usagePercent.toFixed(0) }}%</span>
          </v-progress-circular>

          <div class="summary-copy">
            <span class="text-caption text-medium-emphasis d-block">Memória</span>
            <strong>{{ formatBytes(container.memory.usageBytes) }}</strong>
            <span class="text-caption text-medium-emphasis d-block">
              {{ formatBytes(container.memory.limitBytes) }} disponivel
            </span>
          </div>
        </div>
      </div>

      <v-expand-transition>
        <div v-if="expanded" class="mt-4">
          <div class="mb-4">
            <div class="d-flex align-center justify-space-between mb-2">
              <span class="text-caption text-medium-emphasis">CPU</span>
              <strong>{{ container.cpu.usagePercent.toFixed(1) }}%</strong>
            </div>
            <UsageLineChart
              :values="resolveContainerHistory('cpu', container.cpu.usagePercent)"
              :timestamps="timestamps"
              :range-hours="rangeHours"
              :color="resolveChartColor(resolveUsageColor(container.cpu.usagePercent))"
              :height="84"
            />
          </div>

          <div class="mb-4">
            <div class="d-flex align-center justify-space-between mb-2">
              <span class="text-caption text-medium-emphasis">Memória</span>
              <strong>{{ container.memory.usagePercent.toFixed(1) }}%</strong>
            </div>
            <UsageLineChart
              :values="resolveContainerHistory('memory', container.memory.usagePercent)"
              :timestamps="timestamps"
              :range-hours="rangeHours"
              :color="resolveChartColor(resolveUsageColor(container.memory.usagePercent))"
              :height="84"
              :raw-values="resolveContainerRawMemoryHistory()"
              :raw-formatter="formatBytes"
            />
            <p class="text-caption text-medium-emphasis mt-2 mb-0">
              {{ formatBytes(container.memory.usageBytes) }} / {{ formatBytes(container.memory.limitBytes) }}
            </p>
          </div>

          <v-divider class="my-3" />

          <div class="resource-grid resource-grid--details">
            <div class="resource-metric">
              <span class="text-caption text-medium-emphasis">Rede RX/TX</span>
              <strong>{{ formatBytes(container.network.rxBytes) }} / {{ formatBytes(container.network.txBytes) }}</strong>
            </div>

            <div class="resource-metric">
              <span class="text-caption text-medium-emphasis">Disco R/W</span>
              <strong>{{ formatBytes(container.blockIo.readBytes) }} / {{ formatBytes(container.blockIo.writeBytes) }}</strong>
            </div>

            <div class="resource-metric">
              <span class="text-caption text-medium-emphasis">PIDs</span>
              <strong>{{ container.pids ?? 'N/A' }}</strong>
            </div>
          </div>
        </div>
      </v-expand-transition>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import type { DockerContainerResourceMetrics, ResourceHistoryPoint } from '@/types/api'
import { computed, ref } from 'vue'
import { formatBytes } from '@/utils/format'
import UsageLineChart from './UsageLineChart.vue'

const props = defineProps<{
  container: DockerContainerResourceMetrics
  historyPoints: ResourceHistoryPoint[]
  rangeHours?: number
}>()

const expanded = ref(false)
const timestamps = computed(() => props.historyPoints.map((point) => point.timestamp))
const statusColor = computed(() => resolveStatusColor(props.container.status))

function toggleExpanded(): void {
  expanded.value = !expanded.value
}

function resolveUsageColor(percentage: number): string {
  if (percentage >= 85) return 'error'
  if (percentage >= 65) return 'warning'
  return 'success'
}

function resolveStatusColor(status: string): string {
  const normalized = status.toLowerCase()
  if (normalized.includes('running') || normalized === 'up') return 'success'
  if (normalized.includes('paused')) return 'warning'
  if (normalized.includes('exited') || normalized.includes('dead')) return 'error'
  return 'primary'
}

function resolveContainerHistory(metric: 'cpu' | 'memory', fallbackValue: number): number[] {
  const values = props.historyPoints.map((point) =>
    metric === 'cpu' ? point.cpuUsagePercent : point.memoryUsagePercent
  )

  if (values.length >= 2) {
    return values
  }

  return [0, fallbackValue]
}

function resolveContainerRawMemoryHistory(): number[] {
  const values = props.historyPoints.map((point) => point.memoryUsedBytes)
  if (values.length >= 2) return values
  return [0, props.container.memory.usageBytes]
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
}

.resource-grid--details {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.resource-summary {
  display: grid;
  gap: 12px;
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.summary-metric {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  min-width: 0;
  padding: 12px 14px;
  border-radius: 14px;
  background: rgba(var(--v-theme-primary), 0.05);
  border: 1px solid rgba(var(--v-theme-primary), 0.08);
}

.summary-copy {
  flex: 1;
  min-width: 0;
}

.resource-metric {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.resource-toggle-btn {
  min-width: auto;
  padding-inline: 8px;
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

  .resource-summary {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>