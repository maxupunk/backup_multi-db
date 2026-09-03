<template>
  <v-card variant="outlined">
    <v-card-text class="pa-4">
      <div class="d-flex align-center justify-space-between mb-2">
        <span class="text-subtitle-2 font-weight-bold text-truncate">{{ network.name }}</span>
        <div class="d-flex ga-1">
          <v-chip label size="x-small" variant="tonal">{{ network.driver }}</v-chip>
          <v-chip label size="x-small" variant="tonal">{{ network.scope }}</v-chip>
        </div>
      </div>

      <div v-if="subnet" class="text-caption text-medium-emphasis mb-1">
        <v-icon icon="mdi-ip-network-outline" size="14" />
        {{ subnet }}
      </div>

      <div class="text-caption text-medium-emphasis mb-3">
        <v-icon icon="mdi-server-outline" size="14" />
        {{ containerCount }} conectado(s) · {{ runningCount }} em execução
      </div>

      <div class="d-flex justify-end">
        <v-btn
          density="compact"
          prepend-icon="mdi-information-outline"
          size="small"
          variant="tonal"
          @click="emit('detail', network)"
        >
          Detalhes
        </v-btn>
        <v-btn
          class="ml-1"
          color="error"
          density="compact"
          :disabled="loading || runningCount > 0"
          icon="mdi-delete-outline"
          size="small"
          :title="removeTitle"
          variant="text"
          @click="emit('remove', network)"
        >
          <v-icon />
        </v-btn>
      </div>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { DockerNetworkSummary } from '@/types/api'

const props = defineProps<{ network: DockerNetworkSummary; loading?: boolean }>()
const emit = defineEmits<{
  (e: 'detail', n: DockerNetworkSummary): void
  (e: 'remove', n: DockerNetworkSummary): void
}>()

const subnet = computed(() => props.network.ipam.config[0]?.subnet ?? null)
const containerCount = computed(() => props.network.connectedContainers)
const runningCount = computed(() => props.network.runningContainers ?? containerCount.value)
const removeTitle = computed(() => {
  if (runningCount.value === 0) return 'Remover rede'
  const names = props.network.runningContainerNames?.join(', ')
  return names
    ? `Em uso por contêineres em execução: ${names}`
    : 'Pare os contêineres em execução antes de remover a rede'
})
</script>
