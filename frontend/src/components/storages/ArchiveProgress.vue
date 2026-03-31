<template>
  <v-card class="mb-4" :loading="loading">
    <v-card-title class="d-flex align-center">
      <v-icon class="mr-2" color="secondary" icon="mdi-archive-arrow-down" />
      Geração de Archive
      <v-spacer />
      <v-chip :color="statusColor" label size="small">
        {{ statusLabel }}
      </v-chip>
    </v-card-title>

    <v-card-text>
      <div v-if="job">
        <div class="d-flex justify-space-between text-body-2 mb-1">
          <span>{{ job.processedFiles }} / {{ job.totalFiles ?? '?' }} arquivos</span>
        </div>

        <v-progress-linear
          :color="statusColor"
          height="8"
          :indeterminate="job.status === 'building' && !job.totalFiles"
          :model-value="progressPercent"
          rounded
        />

        <div v-if="job.error" class="text-error text-caption mt-2">
          {{ job.error }}
        </div>

        <div v-if="job.status === 'ready'" class="mt-4">
          <v-btn
            color="success"
            :loading="downloading"
            prepend-icon="mdi-download"
            variant="flat"
            @click="downloadArchive"
          >
            Baixar .tar.gz
          </v-btn>
          <span v-if="job.expiresAt" class="ml-3 text-caption text-medium-emphasis">
            Expira em {{ formatDateTimePtBR(job.expiresAt) }}
          </span>
        </div>
      </div>

      <div v-else-if="fetchError" class="py-4">
        <v-alert
          density="compact"
          type="warning"
          variant="tonal"
        >
          <div class="d-flex align-center justify-space-between">
            <span>
              Não foi possível obter o status do job
              <span v-if="consecutiveErrors >= MAX_CONSECUTIVE_ERRORS"> (tentativas esgotadas)</span>.
            </span>
            <v-btn
              class="ml-3"
              density="compact"
              prepend-icon="mdi-refresh"
              variant="tonal"
              @click="retryFetch"
            >
              Tentar novamente
            </v-btn>
          </div>
        </v-alert>
      </div>

      <div v-else class="text-center text-medium-emphasis py-4">
        Carregando informações do archive...
      </div>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import type { ArchiveJob } from '@/types/api'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { storagesApi } from '@/services/api'
import { transmit } from '@/plugins/transmit'
import { useNotifier } from '@/composables/useNotifier'
import { formatDateTimePtBR } from '@/utils/format'

const props = defineProps<{
  jobId: string
}>()

const MAX_CONSECUTIVE_ERRORS = 5

const notify = useNotifier()
const job = ref<ArchiveJob | null>(null)
const loading = ref(false)
const downloading = ref(false)
const fetchError = ref(false)
const consecutiveErrors = ref(0)

const TERMINAL_STATUSES = new Set(['ready', 'failed', 'expired'])

let subscription: ReturnType<typeof transmit.subscription> | null = null
let pollInterval: ReturnType<typeof setInterval> | null = null

const progressPercent = computed(() => {
  if (!job.value || !job.value.totalFiles) return 0
  return Math.round((job.value.processedFiles / job.value.totalFiles) * 100)
})

const statusColor = computed(() => {
  if (!job.value) return 'grey'
  const colors: Record<string, string> = {
    pending: 'warning',
    building: 'info',
    ready: 'success',
    expired: 'grey',
    failed: 'error',
  }
  return colors[job.value.status] ?? 'grey'
})

const statusLabel = computed(() => {
  if (!job.value) return 'Carregando'
  const labels: Record<string, string> = {
    pending: 'Pendente',
    building: 'Gerando...',
    ready: 'Pronto',
    expired: 'Expirado',
    failed: 'Falhou',
  }
  return labels[job.value.status] ?? job.value.status
})

async function fetchJob () {
  loading.value = true
  try {
    const response = await storagesApi.getArchiveJob(props.jobId)
    if (response.data) {
      job.value = response.data
      fetchError.value = false
      consecutiveErrors.value = 0
    }
    if (job.value && TERMINAL_STATUSES.has(job.value.status)) {
      stopPolling()
    }
  } catch {
    consecutiveErrors.value++
    fetchError.value = true
    if (consecutiveErrors.value >= MAX_CONSECUTIVE_ERRORS) {
      stopPolling()
    }
  } finally {
    loading.value = false
  }
}

function retryFetch () {
  fetchError.value = false
  consecutiveErrors.value = 0
  fetchJob()
  startPolling()
}

function startPolling () {
  if (pollInterval) return
  pollInterval = setInterval(() => {
    fetchJob()
  }, 3000)
}

function stopPolling () {
  if (pollInterval) {
    clearInterval(pollInterval)
    pollInterval = null
  }
}

async function downloadArchive () {
  downloading.value = true
  try {
    await storagesApi.downloadArchive(props.jobId)
  } catch (error) {
    const msg = error instanceof Error ? error.message : 'Erro ao baixar archive'
    notify(msg, 'error')
  } finally {
    downloading.value = false
  }
}

onMounted(async () => {
  await fetchJob()

  // Start polling as fallback — stopped when SSE or terminal status confirmed
  startPolling()

  try {
    subscription = transmit.subscription(`notifications/storage-archive/${props.jobId}`)
    await subscription.create()
    subscription.onMessage<Partial<ArchiveJob>>((data) => {
      if (job.value) {
        Object.assign(job.value, data)
        fetchError.value = false
        if (TERMINAL_STATUSES.has(job.value.status)) {
          stopPolling()
        }
      }
    })
  } catch (error) {
    console.error('[ArchiveProgress] Erro SSE:', error)
    // Polling remains active as fallback
  }
})

onUnmounted(async () => {
  stopPolling()
  if (subscription) {
    try { await subscription.delete() } catch { /* ignore */ }
    subscription = null
  }
})
</script>
