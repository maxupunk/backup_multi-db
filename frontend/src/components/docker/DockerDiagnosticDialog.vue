<template>
  <v-dialog v-model="model" max-width="760" scrollable>
    <v-card>
      <v-card-title class="d-flex align-center pa-4">
        <v-icon class="mr-2" icon="mdi-stethoscope" />
        Diagnóstico de rede
        <v-spacer />
        <v-chip v-if="job" :color="statusColor" label size="small" variant="tonal">
          {{ statusLabel }}
        </v-chip>
      </v-card-title>
      <v-divider />

      <v-card-text class="pa-4" style="max-height: 640px">
        <div class="text-body-2 mb-4">
          Execute ping, teste de porta e curl em background, com saída em tempo real via SSE.
          <span v-if="preset?.contextLabel" class="text-medium-emphasis">
            Contexto: {{ preset.contextLabel }}.
          </span>
        </div>

        <v-tabs v-model="tool" class="mb-4" color="primary" density="comfortable">
          <v-tab value="ping" prepend-icon="mdi-pulse">Ping</v-tab>
          <v-tab value="port_scan" prepend-icon="mdi-lan-check">Scan de porta</v-tab>
          <v-tab value="curl" prepend-icon="mdi-web">Curl</v-tab>
        </v-tabs>

        <div v-if="suggestedTargets.length" class="mb-4">
          <div class="text-caption font-weight-bold mb-2">Alvos sugeridos</div>
          <div class="d-flex flex-wrap ga-2">
            <v-chip
              v-for="option in suggestedTargets"
              :key="`${option.label}-${option.value}`"
              :color="selectedSuggestedValue === option.value ? 'primary' : undefined"
              label
              size="small"
              variant="tonal"
              @click="selectTarget(option.value)"
            >
              {{ option.label }}
            </v-chip>
          </div>
        </div>

        <v-row>
          <v-col v-if="tool !== 'curl'" cols="12" md="8">
            <v-text-field
              v-model="target"
              density="comfortable"
              hint="Aceita hostname do container, domínio ou IP"
              label="Host ou IP"
              persistent-hint
              prepend-inner-icon="mdi-crosshairs-gps"
              variant="outlined"
            />
          </v-col>

          <v-col v-if="tool === 'ping'" cols="12" md="4">
            <v-text-field
              v-model.number="count"
              density="comfortable"
              label="Tentativas"
              max="10"
              min="1"
              prepend-inner-icon="mdi-counter"
              type="number"
              variant="outlined"
            />
          </v-col>

          <template v-else-if="tool === 'port_scan'">
            <v-col cols="12" md="4">
              <v-text-field
                v-model.number="port"
                density="comfortable"
                label="Porta"
                max="65535"
                min="1"
                prepend-inner-icon="mdi-ethernet"
                type="number"
                variant="outlined"
              />
            </v-col>
            <v-col cols="12" md="4">
              <v-text-field
                v-model.number="timeoutMs"
                density="comfortable"
                label="Timeout (ms)"
                max="10000"
                min="200"
                prepend-inner-icon="mdi-timer-outline"
                type="number"
                variant="outlined"
              />
            </v-col>
          </template>

          <template v-else>
            <v-col cols="12" md="8">
              <v-text-field
                v-model="curlUrl"
                density="comfortable"
                hint="Ex.: http://container:8080/health"
                label="URL"
                persistent-hint
                prepend-inner-icon="mdi-web"
                variant="outlined"
              />
            </v-col>

            <v-col cols="12" md="4">
              <v-text-field
                v-model.number="timeoutMs"
                density="comfortable"
                label="Timeout (ms)"
                max="10000"
                min="200"
                prepend-inner-icon="mdi-timer-outline"
                type="number"
                variant="outlined"
              />
            </v-col>
          </template>
        </v-row>

        <div class="d-flex justify-end mb-4">
          <v-btn
            :disabled="!canRun"
            :loading="submitting"
            color="primary"
            :prepend-icon="runButtonIcon"
            @click="runDiagnostic"
          >
            {{ runButtonLabel }}
          </v-btn>
        </div>

        <template v-if="job">
          <v-divider class="mb-4" />

          <div class="d-flex flex-wrap ga-2 mb-3">
            <v-chip label size="small" variant="tonal">
              {{ job.tool === 'curl' ? 'URL' : 'Alvo' }}: {{ job.target }}
            </v-chip>
            <v-chip v-if="job.port !== null" label size="small" variant="tonal">
              Porta {{ job.port }}
            </v-chip>
            <v-chip v-if="job.latencyMs !== null" label size="small" variant="tonal">
              {{ job.latencyMs }} ms
            </v-chip>
            <v-chip
              v-if="canOpenCurlFromCurrentJob"
              class="diagnostic-chip-action"
              color="success"
              label
              prepend-icon="mdi-web"
              size="small"
              variant="tonal"
              @click="openCurlFromCurrentJob"
            >
              Porta aberta • abrir curl
            </v-chip>
            <v-chip
              v-else-if="job.tool === 'port_scan' && job.portOpen === false"
              color="warning"
              label
              size="small"
              variant="tonal"
            >
              Porta fechada
            </v-chip>
          </div>

          <v-alert
            v-if="job.summary || job.error"
            class="mb-4"
            density="compact"
            :type="job.status === 'failed' ? 'error' : 'info'"
            variant="tonal"
          >
            {{ job.error ?? job.summary }}
          </v-alert>

          <v-progress-linear
            v-if="isRunning"
            class="mb-4"
            color="info"
            indeterminate
            rounded
          />

          <v-sheet border class="pa-3 rounded diagnostic-log-surface" color="grey-lighten-5">
            <div class="d-flex align-center justify-space-between mb-2">
              <div class="text-caption font-weight-bold">Saída em tempo real</div>
              <v-btn
                density="compact"
                prepend-icon="mdi-refresh"
                variant="text"
                @click="refreshJob"
              >
                Atualizar
              </v-btn>
            </div>

            <div ref="logContainer" class="diagnostic-log">
              <div v-if="job.outputLines.length === 0" class="text-caption text-medium-emphasis">
                Aguardando saída do diagnóstico...
              </div>
              <div
                v-for="(line, index) in job.outputLines"
                :key="`${job.id}-${index}-${line}`"
                class="diagnostic-log__line text-caption text-monospace"
              >
                {{ line }}
              </div>
            </div>
          </v-sheet>
        </template>
      </v-card-text>

      <v-divider />
      <v-card-actions class="justify-end pa-3">
        <v-btn variant="text" @click="model = false">Fechar</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { useNotifier } from '@/composables/useNotifier'
import { transmit } from '@/plugins/transmit'
import { dockerDiagnosticsApi } from '@/services/dockerService'
import type {
  DockerDiagnosticJob,
  DockerDiagnosticPreset,
  DockerDiagnosticStartPayload,
  DockerDiagnosticTool,
} from '@/types/api'

const DEFAULT_PING_COUNT = 4
const DEFAULT_TIMEOUT_MS = 2000

const model = defineModel<boolean>({ default: false })
const props = defineProps<{
  preset?: DockerDiagnosticPreset | null
}>()

const notify = useNotifier()
const tool = ref<DockerDiagnosticTool>('ping')
const target = ref('')
const curlUrl = ref('')
const port = ref<number | null>(null)
const count = ref(DEFAULT_PING_COUNT)
const timeoutMs = ref(DEFAULT_TIMEOUT_MS)
const submitting = ref(false)
const job = ref<DockerDiagnosticJob | null>(null)
const currentJobId = ref<string | null>(null)
const logContainer = ref<HTMLElement | null>(null)

let subscription: ReturnType<typeof transmit.subscription> | null = null

const suggestedTargets = computed(() => props.preset?.suggestedTargets ?? [])
const selectedSuggestedValue = computed(() => {
  if (tool.value === 'curl') {
    return suggestedTargets.value.find((option) => normalizeCurlUrl(option.value) === curlUrl.value)?.value
  }

  return suggestedTargets.value.find((option) => option.value === target.value)?.value
})

const canRun = computed(() => {
  const inputValue = tool.value === 'curl' ? curlUrl.value : target.value

  if (!inputValue.trim()) {
    return false
  }

  if (tool.value === 'port_scan') {
    return port.value !== null && port.value >= 1 && port.value <= 65535
  }

  if (tool.value === 'curl') {
    return true
  }

  return count.value >= 1 && count.value <= 10
})

const isRunning = computed(() => {
  return job.value?.status === 'pending' || job.value?.status === 'running'
})

const canOpenCurlFromCurrentJob = computed(() => {
  return (
    job.value?.tool === 'port_scan' &&
    job.value.portOpen === true &&
    job.value.port !== null
  )
})

const runButtonLabel = computed(() => {
  const labels: Record<DockerDiagnosticTool, string> = {
    ping: 'Executar ping',
    port_scan: 'Testar porta',
    curl: 'Executar curl',
  }

  return labels[tool.value]
})

const runButtonIcon = computed(() => {
  const icons: Record<DockerDiagnosticTool, string> = {
    ping: 'mdi-pulse',
    port_scan: 'mdi-lan-check',
    curl: 'mdi-web',
  }

  return icons[tool.value]
})

const statusColor = computed(() => {
  const colors: Record<string, string> = {
    pending: 'warning',
    running: 'info',
    completed: 'success',
    failed: 'error',
  }
  return colors[job.value?.status ?? 'pending'] ?? 'grey'
})

const statusLabel = computed(() => {
  const labels: Record<string, string> = {
    pending: 'Pendente',
    running: 'Executando',
    completed: 'Concluído',
    failed: 'Falhou',
  }
  return labels[job.value?.status ?? 'pending'] ?? 'Pendente'
})

function applyPreset(preset: DockerDiagnosticPreset | null | undefined) {
  tool.value = preset?.tool ?? 'ping'
  target.value = preset?.target ?? ''
  curlUrl.value =
    preset?.tool === 'curl'
      ? normalizeCurlUrl(preset.target ?? '')
      : buildCurlUrl(preset?.target ?? '', preset?.port ?? null)
  port.value = preset?.port ?? null
  count.value = preset?.count ?? DEFAULT_PING_COUNT
  timeoutMs.value = preset?.timeoutMs ?? DEFAULT_TIMEOUT_MS
  job.value = null
  currentJobId.value = null
}

function normalizeCurlUrl(value: string): string {
  const trimmedValue = value.trim()
  if (!trimmedValue) {
    return ''
  }

  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmedValue)) {
    return trimmedValue
  }

  return `http://${trimmedValue}`
}

function buildCurlUrl(value: string, portValue: number | null): string {
  const normalizedValue = normalizeCurlUrl(value)

  if (!normalizedValue || portValue === null) {
    return normalizedValue
  }

  try {
    const url = new URL(normalizedValue)
    if (!url.port) {
      url.port = String(portValue)
    }
    return url.toString()
  } catch {
    return normalizedValue
  }
}

function selectTarget(value: string) {
  if (tool.value === 'curl') {
    curlUrl.value = buildCurlUrl(value, port.value)
    return
  }

  target.value = value
}

function openCurlFromCurrentJob() {
  if (!canOpenCurlFromCurrentJob.value || !job.value || job.value.port === null) {
    return
  }

  curlUrl.value = buildCurlUrl(job.value.target, job.value.port)
  port.value = job.value.port
  tool.value = 'curl'
}

async function ensureSubscription(jobId: string) {
  await disposeSubscription()

  try {
    subscription = transmit.subscription(`notifications/docker-diagnostics/${jobId}`)
    await subscription.create()
    subscription.onMessage<DockerDiagnosticJob>((data) => {
      job.value = {
        ...data,
        outputLines: [...data.outputLines],
      }
    })
  } catch (error) {
    console.error('[DockerDiagnosticDialog] Erro SSE:', error)
    notify('Não foi possível acompanhar o diagnóstico em tempo real.', 'warning')
  }
}

async function disposeSubscription() {
  if (!subscription) {
    return
  }

  try {
    await subscription.delete()
  } catch {
    // ignore
  }

  subscription = null
}

async function refreshJob() {
  if (!currentJobId.value) {
    return
  }

  try {
    job.value = await dockerDiagnosticsApi.getJob(currentJobId.value)
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Erro ao buscar job de diagnóstico'
    notify(message, 'error')
  }
}

async function runDiagnostic() {
  if (!canRun.value) {
    return
  }

  const payload: DockerDiagnosticStartPayload = {
    tool: tool.value,
    target: tool.value === 'curl' ? curlUrl.value.trim() : target.value.trim(),
    timeoutMs: timeoutMs.value,
  }

  if (tool.value === 'ping') {
    payload.count = count.value
  } else if (tool.value === 'port_scan') {
    payload.port = port.value ?? undefined
  }

  submitting.value = true

  try {
    const createdJob = await dockerDiagnosticsApi.start(payload)
    currentJobId.value = createdJob.id
    job.value = createdJob

    await ensureSubscription(createdJob.id)
    await refreshJob()
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Falha ao iniciar diagnóstico'
    notify(message, 'error')
  } finally {
    submitting.value = false
  }
}

watch(
  () => props.preset,
  (preset) => {
    applyPreset(preset)
    void disposeSubscription()
  },
  { deep: true, immediate: true }
)

watch(model, async (opened) => {
  if (!opened) {
    await disposeSubscription()
    return
  }

  if (currentJobId.value) {
    await ensureSubscription(currentJobId.value)
    await refreshJob()
  }
})

watch(tool, (nextTool) => {
  if (nextTool === 'curl' && !curlUrl.value.trim()) {
    curlUrl.value = buildCurlUrl(target.value, port.value)
  }
})

watch(
  () => job.value?.outputLines.length,
  async () => {
    await nextTick()
    if (logContainer.value) {
      logContainer.value.scrollTop = logContainer.value.scrollHeight
    }
  }
)

onUnmounted(async () => {
  await disposeSubscription()
})
</script>

<style scoped>
.diagnostic-log-surface {
  border: 1px solid rgba(0, 0, 0, 0.08);
}

.diagnostic-log {
  max-height: 260px;
  overflow-y: auto;
}

.diagnostic-log__line + .diagnostic-log__line {
  margin-top: 6px;
}

.diagnostic-chip-action {
  cursor: pointer;
}
</style>