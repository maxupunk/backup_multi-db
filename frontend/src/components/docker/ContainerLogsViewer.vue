<template>
  <div>
    <div class="d-flex align-center flex-wrap ga-2 mb-3">
      <v-select
        v-model="tailOption"
        :disabled="hasDateFilter"
        density="compact"
        hide-details
        :items="TAIL_OPTIONS"
        label="Linhas"
        style="max-width: 120px"
        variant="outlined"
      />

      <v-checkbox
        v-model="showTimestamps"
        density="compact"
        hide-details
        label="Exibir data e hora"
      />

      <v-checkbox
        v-model="filterStderr"
        color="error"
        density="compact"
        hide-details
        label="Apenas erros"
      />

      <v-spacer />

      <v-btn
        color="primary"
        density="compact"
        :loading="downloading"
        prepend-icon="mdi-download"
        variant="tonal"
        @click="downloadAllLogs"
      >
        Baixar tudo
      </v-btn>

      <v-btn
        density="compact"
        :loading="loading"
        prepend-icon="mdi-refresh"
        variant="tonal"
        @click="load"
      >
        Atualizar
      </v-btn>
    </div>

    <div class="d-flex align-center flex-wrap ga-2 mb-3">
      <v-text-field
        v-model="sinceDateTime"
        clearable
        density="compact"
        hide-details="auto"
        label="De"
        style="max-width: 220px"
        type="datetime-local"
        variant="outlined"
      />

      <v-text-field
        v-model="untilDateTime"
        clearable
        density="compact"
        hide-details="auto"
        label="Até"
        style="max-width: 220px"
        type="datetime-local"
        variant="outlined"
      />

      <v-btn
        density="compact"
        :disabled="loading || !!dateRangeError"
        prepend-icon="mdi-calendar-search"
        variant="tonal"
        @click="applyDateFilter"
      >
        Aplicar período
      </v-btn>

      <v-btn
        density="compact"
        :disabled="loading || !hasDateFilter"
        prepend-icon="mdi-filter-off"
        variant="text"
        @click="clearDateFilter"
      >
        Limpar período
      </v-btn>
    </div>

    <v-alert
      v-if="requestError"
      class="mb-3"
      density="compact"
      type="error"
      variant="tonal"
    >
      {{ requestError }}
    </v-alert>

    <div
      ref="logBox"
      class="log-box pa-3 rounded font-weight-regular text-caption"
      style="background: #0d1117; color: #c9d1d9; height: 480px; overflow-y: auto; white-space: pre-wrap; word-break: break-all; font-family: monospace"
    >
      <div
        v-for="(entry, i) in filtered"
        :key="i"
        :class="entry.stream === 'stderr' ? 'text-red-lighten-2' : ''"
      >
        <span v-if="showTimestamps" class="text-blue-grey-lighten-3 mr-1">
          {{ formatTimestamp(entry.timestamp) }}
        </span>{{ entry.message }}
      </div>

      <div v-if="filtered.length === 0 && !loading" class="text-medium-emphasis py-4 text-center">
        Nenhum log disponível.
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import type { DockerLogEntry, DockerLogsParams } from '@/types/api'
import { dockerContainersApi } from '@/services/dockerService'

const TAIL_OPTIONS = [
  { title: '50 linhas', value: 50 },
  { title: '200 linhas', value: 200 },
  { title: '500 linhas', value: 500 },
  { title: 'Tudo', value: 'all' as const },
]

const props = defineProps<{ containerId: string }>()

const entries = ref<DockerLogEntry[]>([])
const loading = ref(false)
const downloading = ref(false)
const tailOption = ref<number | 'all'>(200)
const showTimestamps = ref(false)
const filterStderr = ref(false)
const sinceDateTime = ref('')
const untilDateTime = ref('')
const requestError = ref<string | null>(null)
const logBox = ref<HTMLElement | null>(null)

const filtered = computed(() =>
  filterStderr.value ? entries.value.filter((e) => e.stream === 'stderr') : entries.value
)

const hasDateFilter = computed(() => Boolean(sinceDateTime.value || untilDateTime.value))

const dateRangeError = computed(() => {
  const since = parseDateTimeInput(sinceDateTime.value)
  const until = parseDateTimeInput(untilDateTime.value)

  if (sinceDateTime.value && since === undefined) {
    return 'Informe uma data/hora inicial válida.'
  }

  if (untilDateTime.value && until === undefined) {
    return 'Informe uma data/hora final válida.'
  }

  if (since !== undefined && until !== undefined && since > until) {
    return 'A data/hora inicial deve ser anterior à final.'
  }

  return null
})

function parseDateTimeInput(value: string): number | undefined {
  if (!value) return undefined

  const date = new Date(value)
  const timestamp = date.getTime()

  if (Number.isNaN(timestamp)) return undefined

  return Math.floor(timestamp / 1000)
}

function buildRequestParams(): DockerLogsParams {
  return {
    tail: hasDateFilter.value ? 'all' : tailOption.value,
    since: parseDateTimeInput(sinceDateTime.value),
    until: parseDateTimeInput(untilDateTime.value),
    timestamps: true,
  }
}

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Falha ao carregar os logs.'
}

async function load() {
  if (dateRangeError.value) {
    requestError.value = dateRangeError.value
    entries.value = []
    return
  }

  loading.value = true
  requestError.value = null

  try {
    entries.value = await dockerContainersApi.getLogs(props.containerId, buildRequestParams())
    await nextTick()

    if (logBox.value) {
      logBox.value.scrollTop = hasDateFilter.value ? 0 : logBox.value.scrollHeight
    }
  } catch (error) {
    entries.value = []
    requestError.value = getErrorMessage(error)
  } finally {
    loading.value = false
  }
}

async function downloadAllLogs() {
  downloading.value = true
  requestError.value = null

  try {
    await dockerContainersApi.downloadAllLogs(props.containerId)
  } catch (error) {
    requestError.value = getErrorMessage(error)
  } finally {
    downloading.value = false
  }
}

function applyDateFilter() {
  void load()
}

function clearDateFilter() {
  sinceDateTime.value = ''
  untilDateTime.value = ''
  void load()
}

function formatTimestamp(ts: string): string {
  if (!ts) return ''

  try {
    return new Date(ts).toLocaleString('pt-BR', { hour12: false })
  } catch {
    return ts
  }
}

watch(tailOption, () => {
  void load()
})

watch(
  () => props.containerId,
  () => {
    void load()
  }
)

onMounted(() => {
  void load()
})
</script>
