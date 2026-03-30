<template>
  <v-card class="mb-4" :loading="loading">
    <v-card-title class="d-flex align-center">
      <v-icon class="mr-2" color="primary" icon="mdi-content-copy" />
      Job de Cópia
      <v-spacer />
      <v-chip :color="statusColor" label size="small">
        {{ statusLabel }}
      </v-chip>
    </v-card-title>

    <v-card-text>
      <div v-if="job" class="mb-3">
        <div class="d-flex justify-space-between text-body-2 mb-1">
          <span>{{ job.filesTransferred }} arquivos transferidos</span>
          <span>{{ formatFileSize(job.bytesTransferred) }}</span>
        </div>

        <v-progress-linear
          :color="statusColor"
          height="8"
          :indeterminate="job.status === 'running' && !job.totalFiles"
          :model-value="progressPercent"
          rounded
        />

        <div v-if="job.error" class="text-error text-caption mt-2">
          {{ job.error }}
        </div>
      </div>

      <div v-else class="text-center text-medium-emphasis py-4">
        Carregando informações do job...
      </div>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import type { CopyJob } from '@/types/api'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { storagesApi } from '@/services/api'
import { transmit } from '@/plugins/transmit'
import { formatFileSize } from '@/utils/format'

const props = defineProps<{
  jobId: string
}>()

const job = ref<CopyJob | null>(null)
const loading = ref(false)

let subscription: ReturnType<typeof transmit.subscription> | null = null

const progressPercent = computed(() => {
  if (!job.value || !job.value.totalFiles) return 0
  return Math.round((job.value.filesTransferred / job.value.totalFiles) * 100)
})

const statusColor = computed(() => {
  if (!job.value) return 'grey'
  const colors: Record<string, string> = {
    pending: 'warning',
    running: 'info',
    completed: 'success',
    failed: 'error',
  }
  return colors[job.value.status] ?? 'grey'
})

const statusLabel = computed(() => {
  if (!job.value) return 'Carregando'
  const labels: Record<string, string> = {
    pending: 'Pendente',
    running: 'Copiando',
    completed: 'Concluído',
    failed: 'Falhou',
  }
  return labels[job.value.status] ?? job.value.status
})

async function fetchJob () {
  loading.value = true
  try {
    const response = await storagesApi.getCopyJob(props.jobId)
    if (response.data) job.value = response.data
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await fetchJob()

  try {
    subscription = transmit.subscription(`notifications/storage-copy/${props.jobId}`)
    await subscription.create()
    subscription.onMessage<Partial<CopyJob>>((data) => {
      if (job.value) {
        Object.assign(job.value, data)
      }
    })
  } catch (error) {
    console.error('[CopyJobProgress] Erro SSE:', error)
  }
})

onUnmounted(async () => {
  if (subscription) {
    try { await subscription.delete() } catch { /* ignore */ }
    subscription = null
  }
})
</script>
