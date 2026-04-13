<template>
  <div>
    <v-row align="center" class="mb-6">
      <v-col>
        <h1 class="font-weight-bold mb-1 text-h4">Docker Manager</h1>
        <p class="text-body-2 text-medium-emphasis">
          Visão geral do ambiente Docker
        </p>
      </v-col>
      <v-col cols="auto">
        <v-btn :loading="loading" prepend-icon="mdi-refresh" variant="tonal" @click="load">
          Atualizar
        </v-btn>
      </v-col>
    </v-row>

    <DockerUnavailableBanner v-if="unavailable" />

    <template v-else>
    <v-row>
      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/containers" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="success" rounded="lg" size="48">
              <v-icon icon="mdi-cube-outline" />
            </v-avatar>
            <div>
              <div class="text-h5 font-weight-bold">{{ running }}</div>
              <div class="text-caption text-medium-emphasis">Containers em execução</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/containers" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="error" rounded="lg" size="48">
              <v-icon icon="mdi-stop-circle-outline" />
            </v-avatar>
            <div>
              <div class="text-h5 font-weight-bold">{{ stopped }}</div>
              <div class="text-caption text-medium-emphasis">Containers parados</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/volumes" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="primary" rounded="lg" size="48">
              <v-icon icon="mdi-database-outline" />
            </v-avatar>
            <div>
              <div class="text-h5 font-weight-bold">{{ volumes }}</div>
              <div class="text-caption text-medium-emphasis">Volumes</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/images" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="info" rounded="lg" size="48">
              <v-icon icon="mdi-layers-outline" />
            </v-avatar>
            <div>
              <div class="text-h5 font-weight-bold">{{ images }}</div>
              <div class="text-caption text-medium-emphasis">Imagens</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <!-- History range selector -->
    <v-row class="mb-2">
      <v-col cols="12">
        <v-card class="history-filter-card">
          <v-card-text class="px-4 py-3">
            <div class="d-flex flex-column flex-md-row align-md-center justify-space-between ga-3">
              <div>
                <div class="d-flex align-center ga-2">
                  <v-icon color="primary" icon="mdi-chart-timeline-variant" size="20" />
                  <strong>Período dos gráficos</strong>
                </div>
                <p class="text-caption text-medium-emphasis mb-0 mt-1">
                  Dados persistidos por até {{ resourceHistory.retentionDays.value }} dias.
                </p>
              </div>

              <div class="d-flex align-center ga-2">
                <v-progress-circular
                  v-if="resourceHistory.loading.value"
                  color="primary"
                  indeterminate
                  size="18"
                  width="2"
                />

                <v-btn-toggle
                  v-model="selectedHistoryRangeHours"
                  color="primary"
                  density="comfortable"
                  divided
                  mandatory
                >
                  <v-btn
                    v-for="option in historyRangeOptions"
                    :key="option.hours"
                    :value="option.hours"
                    :disabled="resourceHistory.loading.value"
                    variant="text"
                  >
                    {{ option.label }}
                  </v-btn>
                </v-btn-toggle>
              </div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <SystemResourceCharts
      :system="liveSystem"
      :history="resourceHistory.systemHistory.value"
      :range-hours="selectedHistoryRangeHours"
    />
    <DockerContainerResourceCharts
      :overview="dockerOverview"
      :history-by-container-id="resourceHistory.containerHistoryById.value"
      :range-hours="selectedHistoryRangeHours"
      :loading="dockerLoading"
      :error="dockerError"
    />

    </template>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue'
import { dockerContainersApi, dockerImagesApi, dockerVolumesApi } from '@/services/dockerService'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import DockerContainerResourceCharts from '@/components/system/DockerContainerResourceCharts.vue'
import SystemResourceCharts from '@/components/system/SystemResourceCharts.vue'
import { useDockerContainerResources } from '@/composables/useDockerContainerResources'
import { useResourceHistory } from '@/composables/useResourceHistory'
import { useSystemResources } from '@/composables/useSystemResources'
import type { DockerContainerGroup, SystemStatus } from '@/types/api'

const loading = ref(false)
const unavailable = ref(false)
const groups = ref<DockerContainerGroup[]>([])
const volumes = ref(0)
const images = ref(0)

const allContainers = computed(() => groups.value.flatMap((g) => g.containers))
const running = computed(() => allContainers.value.filter((c) => c.state === 'running').length)
const stopped = computed(() => allContainers.value.filter((c) => c.state !== 'running').length)

// Real-time metrics via SSE
const { systemResources } = useSystemResources()
const { overview: dockerOverview, loading: dockerLoading, error: dockerError } =
  useDockerContainerResources()
const resourceHistory = useResourceHistory()
const selectedHistoryRangeHours = ref(1)
const historyRangeOptions = [
  { label: '1h', hours: 1 },
  { label: '24h', hours: 24 },
  { label: '7d', hours: 24 * 7 },
  { label: '15d', hours: 24 * 15 },
]

// Last known system status for chart metadata (hostname, architecture, etc.)
const lastSystemMeta = ref<SystemStatus | null>(null)

const liveSystem = computed<SystemStatus | null>(() => {
  const base = lastSystemMeta.value
  if (!base) return null
  if (!systemResources.value) return base
  return {
    ...base,
    resources: {
      cpu: systemResources.value.cpu,
      memory: systemResources.value.memory,
    },
  }
})

async function load() {
  loading.value = true
  unavailable.value = false
  try {
    const [g, v, imgs] = await Promise.all([
      dockerContainersApi.getGroups(),
      dockerVolumesApi.list(),
      dockerImagesApi.list(),
    ])
    groups.value = g
    volumes.value = v.length
    images.value = imgs.length
  } catch {
    unavailable.value = true
  } finally {
    loading.value = false
  }
}

watch(systemResources, (event) => {
  if (!event) return
  // Seed system metadata from the first SSE event
  if (!lastSystemMeta.value) {
    lastSystemMeta.value = {
      hostname: '',
      platform: '',
      architecture: '',
      uptime: 0,
      nodeVersion: '',
      jobs: null,
      resources: { cpu: event.cpu, memory: event.memory },
    } as unknown as SystemStatus
  }
  resourceHistory.appendSystemEvent(event)
})

watch(dockerOverview, (overview) => {
  if (!overview) return
  resourceHistory.appendContainerOverview(overview)
})

watch(selectedHistoryRangeHours, () => {
  void loadResourceHistory()
})

async function loadResourceHistory(): Promise<void> {
  await resourceHistory.load(selectedHistoryRangeHours.value)
}

onMounted(async () => {
  await load()
  await loadResourceHistory()
})
</script>

<style scoped>
.history-filter-card {
  border: 1px solid rgba(var(--v-border-color), 0.08);
}
</style>
