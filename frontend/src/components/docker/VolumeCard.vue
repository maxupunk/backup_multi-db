<template>
  <v-card variant="outlined">
    <v-card-text class="pa-4">
      <div class="d-flex align-center justify-space-between mb-2">
        <span class="text-subtitle-2 font-weight-bold text-truncate">{{ volume.name }}</span>
        <v-chip label size="x-small" variant="tonal">{{ volume.driver }}</v-chip>
      </div>

      <div class="text-caption text-medium-emphasis text-truncate mb-3">
        <v-icon icon="mdi-folder-outline" size="14" />
        {{ volume.mountpoint }}
      </div>

      <div class="d-flex justify-end ga-1">
        <v-btn
          density="compact"
          prepend-icon="mdi-information-outline"
          size="small"
          variant="tonal"
          @click="emit('detail', volume)"
        >
          Detalhes
        </v-btn>
        <v-btn
          color="error"
          density="compact"
          :disabled="loading"
          prepend-icon="mdi-delete-outline"
          size="small"
          variant="tonal"
          @click="emit('remove', volume.name)"
        >
          Remover
        </v-btn>
      </div>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import type { DockerVolumeSummary } from '@/types/api'

defineProps<{ volume: DockerVolumeSummary; loading?: boolean }>()

const emit = defineEmits<{
  (e: 'remove', name: string): void
  (e: 'detail', volume: DockerVolumeSummary): void
}>()
</script>
