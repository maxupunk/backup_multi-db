<template>
  <v-card variant="outlined">
    <v-card-text class="pa-4">
      <div class="d-flex align-center justify-space-between mb-1">
        <span class="text-subtitle-2 font-weight-bold text-truncate">
          {{ primaryTag }}
        </span>
        <v-chip label size="x-small" variant="tonal">{{ formattedSize }}</v-chip>
      </div>

      <div class="text-caption text-medium-emphasis mb-1">
        ID: <span class="text-monospace">{{ shortId }}</span>
      </div>
      <div class="text-caption text-medium-emphasis mb-3">
        Criado: {{ formattedDate }}
      </div>

      <div class="d-flex justify-end ga-1">
        <v-btn
          density="compact"
          prepend-icon="mdi-information-outline"
          size="small"
          variant="tonal"
          @click="emit('detail', image)"
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
          @click="emit('remove', image.id)"
        >
          Remover
        </v-btn>
      </div>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import type { DockerImageSummary } from '@/types/api'

const props = defineProps<{ image: DockerImageSummary; loading?: boolean }>()
const emit = defineEmits<{
  (e: 'remove', id: string): void
  (e: 'detail', image: DockerImageSummary): void
}>()

const primaryTag = computed(() => props.image.repoTags[0] ?? '<none>:<none>')
const shortId = computed(() => props.image.id.replace('sha256:', '').slice(0, 12))
const formattedSize = computed(() => {
  const mb = props.image.size / 1024 / 1024
  return mb >= 1000 ? `${(mb / 1024).toFixed(1)} GB` : `${mb.toFixed(0)} MB`
})
const formattedDate = computed(() =>
  new Date(props.image.created * 1000).toLocaleDateString('pt-BR')
)
</script>
