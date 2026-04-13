<template>
  <v-card class="volume-card" rounded="lg" variant="outlined">
    <!-- Header band -->
    <div class="volume-card__header d-flex align-center pa-3 ga-3">
      <v-avatar color="primary" rounded="lg" size="40">
        <v-icon icon="mdi-database-outline" size="22" />
      </v-avatar>
      <div class="flex-grow-1 overflow-hidden">
        <div class="text-subtitle-2 font-weight-bold text-truncate" :title="volume.name">
          {{ volume.name }}
        </div>
        <div class="d-flex align-center ga-1 mt-1 flex-wrap">
          <v-chip color="primary" label size="x-small" variant="tonal">
            <v-icon icon="mdi-cog-outline" size="10" start />
            {{ volume.driver }}
          </v-chip>
          <v-chip v-if="volume.scope" label size="x-small" variant="tonal">
            {{ volume.scope }}
          </v-chip>
          <v-chip v-if="labelCount > 0" color="secondary" label size="x-small" variant="tonal">
            {{ labelCount }} label{{ labelCount > 1 ? 's' : '' }}
          </v-chip>
        </div>
      </div>
    </div>

    <v-divider />

    <!-- Metadata -->
    <v-card-text class="pa-3">
      <div class="d-flex align-start ga-2 mb-2">
        <v-icon class="mt-0" color="medium-emphasis" icon="mdi-folder-open-outline" size="16" />
        <span class="text-caption text-medium-emphasis mountpoint-text" :title="volume.mountpoint">
          {{ volume.mountpoint }}
        </span>
      </div>
      <div v-if="volume.createdAt" class="d-flex align-center ga-2">
        <v-icon color="medium-emphasis" icon="mdi-clock-outline" size="16" />
        <span class="text-caption text-medium-emphasis">{{ formatDate(volume.createdAt) }}</span>
      </div>
    </v-card-text>

    <v-divider />

    <!-- Actions -->
    <v-card-actions class="pa-2 ga-1 flex-wrap">
      <v-btn
        density="compact"
        icon="mdi-information-outline"
        size="small"
        variant="text"
        @click="emit('detail', volume)"
      >
        <v-icon />
        <v-tooltip activator="parent" location="top">Detalhes</v-tooltip>
      </v-btn>

      <v-spacer />

      <v-btn
        color="success"
        density="compact"
        :disabled="loading"
        prepend-icon="mdi-cloud-upload-outline"
        size="small"
        variant="tonal"
        @click="emit('backup', volume.name)"
      >
        Backup
      </v-btn>
      <v-btn
        color="primary"
        density="compact"
        :disabled="loading"
        prepend-icon="mdi-download-outline"
        size="small"
        variant="tonal"
        @click="emit('export', volume.name)"
      >
        Download
      </v-btn>
      <v-btn
        color="error"
        density="compact"
        :disabled="loading"
        icon="mdi-delete-outline"
        size="small"
        variant="text"
        @click="emit('remove', volume.name)"
      >
        <v-icon />
        <v-tooltip activator="parent" location="top">Remover</v-tooltip>
      </v-btn>
    </v-card-actions>
  </v-card>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { DockerVolumeSummary } from '@/types/api'

const props = defineProps<{ volume: DockerVolumeSummary; loading?: boolean }>()

const emit = defineEmits<{
  (e: 'remove', name: string): void
  (e: 'detail', volume: DockerVolumeSummary): void
  (e: 'export', name: string): void
  (e: 'backup', name: string): void
}>()

const labelCount = computed(() => Object.keys(props.volume.labels ?? {}).length)

function formatDate(iso: string) {
  return new Date(iso).toLocaleDateString('pt-BR', {
    day: '2-digit',
    month: 'short',
    year: 'numeric',
  })
}
</script>

<style scoped>
.volume-card__header {
  background: linear-gradient(135deg, rgba(var(--v-theme-primary), 0.04) 0%, transparent 70%);
}
.mountpoint-text {
  word-break: break-all;
  line-height: 1.3;
}
</style>
