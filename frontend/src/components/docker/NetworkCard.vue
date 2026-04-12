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
        {{ containerCount }} container(s) conectado(s)
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
      </div>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { DockerNetworkSummary } from '@/types/api'

const props = defineProps<{ network: DockerNetworkSummary }>()
const emit = defineEmits<{ (e: 'detail', n: DockerNetworkSummary): void }>()

const subnet = computed(() => props.network.ipam.config[0]?.subnet ?? null)
const containerCount = computed(() => 0)
</script>
