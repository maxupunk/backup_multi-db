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
    <v-row class="mb-4">
      <v-col cols="12" md="3" sm="6">
        <v-card to="/docker/containers" variant="outlined">
          <v-card-text class="d-flex align-center ga-4 pa-5">
            <v-avatar color="success" rounded="lg" size="48">
              <v-icon icon="mdi-cube-outline" />
            </v-avatar>
            <div class="overflow-hidden">
              <div class="text-h5 font-weight-bold">{{ running }}</div>
              <div class="text-caption text-medium-emphasis text-truncate">Containers em execução</div>
              <div v-if="dfData?.containers" class="text-caption text-success mt-1 font-weight-medium">
                {{ formatBytes(dfData.containers.totalSize) }} gravados (RW)
              </div>
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
            <div class="overflow-hidden">
              <div class="text-h5 font-weight-bold">{{ stopped }}</div>
              <div class="text-caption text-medium-emphasis text-truncate">Containers parados</div>
              <div v-if="dfData?.containers?.reclaimableSize" class="text-caption text-error mt-1 font-weight-medium">
                {{ formatBytes(dfData.containers.reclaimableSize) }} liberável
              </div>
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
            <div class="overflow-hidden">
              <div class="text-h5 font-weight-bold">{{ volumes }}</div>
              <div class="text-caption text-medium-emphasis text-truncate">Volumes</div>
              <div v-if="dfData?.volumes" class="text-caption text-primary mt-1 font-weight-medium">
                {{ formatBytes(dfData.volumes.totalSize) }} em disco
              </div>
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
            <div class="overflow-hidden">
              <div class="text-h5 font-weight-bold">{{ images }}</div>
              <div class="text-caption text-medium-emphasis text-truncate">Imagens</div>
              <div v-if="dfData?.images" class="text-caption text-info mt-1 font-weight-medium">
                {{ formatBytes(dfData.images.totalSize) }} em disco
              </div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <!-- Docker Disk Usage Section -->
    <v-row v-if="dfData" class="mb-4">
      <v-col cols="12">
        <v-card variant="outlined" class="docker-df-card">
          <v-card-text class="pa-5">
            <div class="d-flex flex-column flex-md-row align-md-center justify-space-between ga-3 mb-4">
              <div class="d-flex align-center ga-3">
                <v-avatar color="primary" rounded="lg" size="44" variant="tonal">
                  <v-icon icon="mdi-harddisk" size="24" />
                </v-avatar>
                <div>
                  <div class="text-subtitle-1 font-weight-bold">Uso de Espaço em Disco do Docker</div>
                  <div class="text-caption text-medium-emphasis">
                    Detalhamento do armazenamento ocupado por imagens, volumes, contêineres e build cache
                  </div>
                </div>
              </div>

              <div class="d-flex align-center ga-2 flex-wrap">
                <v-chip color="primary" variant="tonal">
                  Total Ocupado: <strong>{{ formatBytes(dfData.totalSize) }}</strong>
                </v-chip>
                <v-chip v-if="dfData.totalReclaimable > 0" color="warning" variant="tonal">
                  Recuperável: <strong>{{ formatBytes(dfData.totalReclaimable) }}</strong>
                </v-chip>
              </div>
            </div>

            <v-row dense>
              <!-- Imagens -->
              <v-col cols="12" sm="6" md="3">
                <v-card class="pa-3 df-item-card" variant="tonal" color="info">
                  <div class="d-flex align-center justify-space-between mb-1">
                    <span class="text-subtitle-2 font-weight-bold d-flex align-center ga-1">
                      <v-icon icon="mdi-layers-outline" size="16" /> Imagens
                    </span>
                    <v-chip size="x-small" label color="info" variant="flat">
                      {{ dfData.images.totalCount }} itens
                    </v-chip>
                  </div>
                  <div class="text-h6 font-weight-bold text-info">
                    {{ formatBytes(dfData.images.totalSize) }}
                  </div>
                  <div class="text-caption text-medium-emphasis d-flex justify-space-between mt-1">
                    <span>Ativas: {{ dfData.images.activeCount }}</span>
                    <span v-if="dfData.images.reclaimableSize > 0" class="text-warning font-weight-medium">
                      {{ formatBytes(dfData.images.reclaimableSize) }} liberável
                    </span>
                  </div>
                </v-card>
              </v-col>

              <!-- Volumes -->
              <v-col cols="12" sm="6" md="3">
                <v-card class="pa-3 df-item-card" variant="tonal" color="primary">
                  <div class="d-flex align-center justify-space-between mb-1">
                    <span class="text-subtitle-2 font-weight-bold d-flex align-center ga-1">
                      <v-icon icon="mdi-database-outline" size="16" /> Volumes
                    </span>
                    <v-chip size="x-small" label color="primary" variant="flat">
                      {{ dfData.volumes.totalCount }} itens
                    </v-chip>
                  </div>
                  <div class="text-h6 font-weight-bold text-primary">
                    {{ formatBytes(dfData.volumes.totalSize) }}
                  </div>
                  <div class="text-caption text-medium-emphasis d-flex justify-space-between mt-1">
                    <span>Em uso: {{ dfData.volumes.activeCount }}</span>
                    <span v-if="dfData.volumes.reclaimableSize > 0" class="text-warning font-weight-medium">
                      {{ formatBytes(dfData.volumes.reclaimableSize) }} liberável
                    </span>
                  </div>
                </v-card>
              </v-col>

              <!-- Containers -->
              <v-col cols="12" sm="6" md="3">
                <v-card class="pa-3 df-item-card" variant="tonal" color="success">
                  <div class="d-flex align-center justify-space-between mb-1">
                    <span class="text-subtitle-2 font-weight-bold d-flex align-center ga-1">
                      <v-icon icon="mdi-cube-outline" size="16" /> Containers (RW)
                    </span>
                    <v-chip size="x-small" label color="success" variant="flat">
                      {{ dfData.containers.totalCount }} itens
                    </v-chip>
                  </div>
                  <div class="text-h6 font-weight-bold text-success">
                    {{ formatBytes(dfData.containers.totalSize) }}
                  </div>
                  <div class="text-caption text-medium-emphasis d-flex justify-space-between mt-1">
                    <span>Rodando: {{ dfData.containers.activeCount }}</span>
                    <span v-if="dfData.containers.reclaimableSize > 0" class="text-warning font-weight-medium">
                      {{ formatBytes(dfData.containers.reclaimableSize) }} liberável
                    </span>
                  </div>
                </v-card>
              </v-col>

              <!-- Build Cache -->
              <v-col cols="12" sm="6" md="3">
                <v-card class="pa-3 df-item-card" variant="tonal" color="secondary">
                  <div class="d-flex align-center justify-space-between mb-1">
                    <span class="text-subtitle-2 font-weight-bold d-flex align-center ga-1">
                      <v-icon icon="mdi-cached" size="16" /> Build Cache
                    </span>
                    <v-chip size="x-small" label color="secondary" variant="flat">
                      {{ dfData.buildCache.totalCount }} itens
                    </v-chip>
                  </div>
                  <div class="text-h6 font-weight-bold text-secondary">
                    {{ formatBytes(dfData.buildCache.totalSize) }}
                  </div>
                  <div class="text-caption text-medium-emphasis d-flex justify-space-between mt-1">
                    <span>Ativos: {{ dfData.buildCache.activeCount }}</span>
                    <span v-if="dfData.buildCache.reclaimableSize > 0" class="text-warning font-weight-medium">
                      {{ formatBytes(dfData.buildCache.reclaimableSize) }} liberável
                    </span>
                  </div>
                </v-card>
              </v-col>
            </v-row>
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
import {
  dockerContainersApi,
  dockerImagesApi,
  dockerSystemApi,
  dockerVolumesApi,
} from '@/services/dockerService'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import DockerContainerResourceCharts from '@/components/system/DockerContainerResourceCharts.vue'
import SystemResourceCharts from '@/components/system/SystemResourceCharts.vue'
import { useDockerContainerResources } from '@/composables/useDockerContainerResources'
import { useResourceHistory } from '@/composables/useResourceHistory'
import { useSystemResources } from '@/composables/useSystemResources'
import { formatBytes } from '@/utils/format'
import type { DockerContainerGroup, DockerSystemDfResponse, SystemStatus } from '@/types/api'

const loading = ref(false)
const unavailable = ref(false)
const groups = ref<DockerContainerGroup[]>([])
const volumes = ref(0)
const images = ref(0)
const dfData = ref<DockerSystemDfResponse | null>(null)

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
    const [g, v, imgs, df] = await Promise.all([
      dockerContainersApi.getGroups(),
      dockerVolumesApi.list(),
      dockerImagesApi.list(),
      dockerSystemApi.getDf().catch(() => null),
    ])
    groups.value = g
    volumes.value = v.length
    images.value = imgs.length
    dfData.value = df
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
      runtimeVersion: '',
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
  await Promise.all([load(), loadResourceHistory()])
})
</script>

<style scoped>
.history-filter-card {
  border: 1px solid rgba(var(--v-border-color), 0.08);
}
.docker-df-card {
  border: 1px solid rgba(var(--v-border-color), 0.12);
}
.df-item-card {
  border: 1px solid rgba(var(--v-border-color), 0.08);
  transition: transform 0.2s, box-shadow 0.2s;
}
.df-item-card:hover {
  transform: translateY(-2px);
}
</style>
