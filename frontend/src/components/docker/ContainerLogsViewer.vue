<template>
  <div>
    <div class="d-flex align-center flex-wrap ga-2 mb-3">
      <v-select
        v-model="tailOption"
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
        label="Timestamps"
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
        density="compact"
        :loading="loading"
        prepend-icon="mdi-refresh"
        variant="tonal"
        @click="load"
      >
        Atualizar
      </v-btn>
    </div>

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
import type { DockerLogEntry } from '@/types/api'
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
const tailOption = ref<number | 'all'>(200)
const showTimestamps = ref(false)
const filterStderr = ref(false)
const logBox = ref<HTMLElement | null>(null)

const filtered = computed(() =>
  filterStderr.value ? entries.value.filter((e) => e.stream === 'stderr') : entries.value
)

async function load() {
  loading.value = true
  try {
    entries.value = await dockerContainersApi.getLogs(props.containerId, {
      tail: tailOption.value,
      timestamps: showTimestamps.value,
    })
    await nextTick()
    if (logBox.value) {
      logBox.value.scrollTop = logBox.value.scrollHeight
    }
  } finally {
    loading.value = false
  }
}

function formatTimestamp(ts: string): string {
  try {
    return new Date(ts).toLocaleTimeString('pt-BR', { hour12: false })
  } catch {
    return ts
  }
}

watch([tailOption, showTimestamps], () => load())
onMounted(load)
</script>
