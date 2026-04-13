<template>
  <v-card :to="`/docker/containers/${container.id}`" class="container-card" variant="outlined">
    <v-card-text class="pa-4">
      <div class="d-flex align-center justify-space-between mb-2">
        <div class="d-flex flex-column" style="min-width: 0">
          <span class="text-subtitle-2 font-weight-bold text-truncate">
            {{ primaryName }}
          </span>
          <span class="text-caption text-medium-emphasis text-truncate mt-1">
            {{ container.image }}
          </span>
        </div>
        <ContainerStatusChip :state="container.state" class="ml-2 flex-shrink-0" />
      </div>

      <!-- Ports -->
      <div v-if="visiblePorts.length" class="d-flex flex-wrap ga-1 mb-3">
        <v-chip
          v-for="port in visiblePorts"
          :key="`${port.PrivatePort}-${port.Type}`"
          label
          size="x-small"
          variant="tonal"
        >
          {{ port.PublicPort ? `${port.PublicPort}:` : '' }}{{ port.PrivatePort }}/{{ port.Type }}
        </v-chip>
      </div>

      <!-- Resource meters -->
      <template v-if="resources && container.state === 'running'">
        <div class="resource-meters mb-3">
          <div class="d-flex align-center justify-space-between mb-1">
            <span class="text-caption text-medium-emphasis">CPU</span>
            <span class="text-caption font-weight-medium">{{ resources.cpu.usagePercent.toFixed(1) }}%</span>
          </div>
          <v-progress-linear
            :model-value="resources.cpu.usagePercent"
            :color="resolveColor(resources.cpu.usagePercent)"
            bg-color="rgba(var(--v-border-color), 0.12)"
            height="4"
            rounded
            class="mb-2"
          />

          <div class="d-flex align-center justify-space-between mb-1">
            <span class="text-caption text-medium-emphasis">Memória</span>
            <span class="text-caption font-weight-medium">{{ resources.memory.usagePercent.toFixed(1) }}%</span>
          </div>
          <v-progress-linear
            :model-value="resources.memory.usagePercent"
            :color="resolveColor(resources.memory.usagePercent)"
            bg-color="rgba(var(--v-border-color), 0.12)"
            height="4"
            rounded
          />
        </div>
      </template>

      <!-- Actions -->
      <div class="d-flex justify-end ga-1">
        <v-btn
          color="success"
          density="compact"
          :disabled="container.state === 'running' || loading"
          icon="mdi-play"
          size="small"
          variant="tonal"
          @click.prevent="emit('start', container.id)"
        >
          <v-icon icon="mdi-play" />
          <v-tooltip activator="parent" location="top">Iniciar</v-tooltip>
        </v-btn>

        <v-btn
          color="error"
          density="compact"
          :disabled="container.state !== 'running' || loading"
          icon="mdi-stop"
          size="small"
          variant="tonal"
          @click.prevent="emit('stop', container.id)"
        >
          <v-icon icon="mdi-stop" />
          <v-tooltip activator="parent" location="top">Parar</v-tooltip>
        </v-btn>

        <v-btn
          color="warning"
          density="compact"
          :disabled="loading"
          icon="mdi-restart"
          size="small"
          variant="tonal"
          @click.prevent="emit('restart', container.id)"
        >
          <v-icon icon="mdi-restart" />
          <v-tooltip activator="parent" location="top">Reiniciar</v-tooltip>
        </v-btn>
      </div>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { DockerContainerResourceMetrics, DockerContainerSummary } from '@/types/api'
import ContainerStatusChip from './ContainerStatusChip.vue'

const props = defineProps<{
  container: DockerContainerSummary
  loading?: boolean
  resources?: DockerContainerResourceMetrics | null
}>()

const emit = defineEmits<{
  (e: 'start', id: string): void
  (e: 'stop', id: string): void
  (e: 'restart', id: string): void
}>()

const primaryName = computed(() => props.container.names[0] ?? props.container.id.slice(0, 12))
const visiblePorts = computed(() => props.container.ports.slice(0, 4))

function resolveColor(percent: number): string {
  if (percent >= 85) return 'error'
  if (percent >= 65) return 'warning'
  return 'success'
}
</script>
