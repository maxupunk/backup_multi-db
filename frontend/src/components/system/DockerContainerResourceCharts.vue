<template>
  <v-row class="mb-6">
    <v-col cols="12">
      <div class="d-flex align-center mb-4">
        <v-icon class="mr-2" color="primary" icon="mdi-docker" />
        <h2 class="text-h6 font-weight-medium">Recursos por Contêiner</h2>
      </div>
    </v-col>

    <v-col v-if="error" cols="12">
      <v-alert type="error" variant="tonal">
        {{ error }}
      </v-alert>
    </v-col>

    <v-col v-else-if="!overview?.dockerAvailable" cols="12">
      <v-alert type="warning" variant="tonal">
        Docker indisponível para coleta de métricas.
        <div v-if="overview?.unavailableReason" class="mt-1 text-caption">
          {{ overview.unavailableReason }}
        </div>
      </v-alert>
    </v-col>

    <v-col v-else-if="loading && !overview" cols="12">
      <v-skeleton-loader type="card@2" />
    </v-col>

    <v-col v-else-if="!containers.length" cols="12">
      <v-alert type="info" variant="tonal">
        Nenhum contêiner em execução no momento.
      </v-alert>
    </v-col>

    <v-col v-for="container in containers" :key="container.containerId" cols="12" md="6">
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

            <v-chip :color="resolveStatusColor(container.status)" label size="small" variant="tonal">
              {{ container.status }}
            </v-chip>
          </div>

          <div class="mb-4">
            <div class="d-flex align-center justify-space-between mb-2">
              <span class="text-caption text-medium-emphasis">CPU</span>
              <strong>{{ container.cpu.usagePercent.toFixed(1) }}%</strong>
            </div>
            <v-progress-linear
              :color="resolveUsageColor(container.cpu.usagePercent)"
              :model-value="container.cpu.usagePercent"
              bg-color="grey-lighten-3"
              height="10"
              rounded
            />
          </div>

          <div class="mb-4">
            <div class="d-flex align-center justify-space-between mb-2">
              <span class="text-caption text-medium-emphasis">Memória</span>
              <strong>{{ container.memory.usagePercent.toFixed(1) }}%</strong>
            </div>
            <v-progress-linear
              :color="resolveUsageColor(container.memory.usagePercent)"
              :model-value="container.memory.usagePercent"
              bg-color="grey-lighten-3"
              height="10"
              rounded
            />
            <p class="text-caption text-medium-emphasis mt-2 mb-0">
              {{ formatBytes(container.memory.usageBytes) }} / {{ formatBytes(container.memory.limitBytes) }}
            </p>
          </div>

          <v-divider class="my-3" />

          <div class="resource-grid">
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
        </v-card-text>
      </v-card>
    </v-col>
  </v-row>
</template>

<script lang="ts" setup>
import type { DockerContainerResourceOverview } from '@/types/api'
import { computed } from 'vue'
import { formatBytes } from '@/utils/format'

const props = defineProps<{
  overview: DockerContainerResourceOverview | null
  loading: boolean
  error: string | null
}>()

const containers = computed(() => props.overview?.containers ?? [])

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

@media (width <= 960px) {
  .resource-grid {
    grid-template-columns: 1fr;
  }
}
</style>
