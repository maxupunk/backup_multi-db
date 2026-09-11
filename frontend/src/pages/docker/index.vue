<template>
  <div>
    <v-row align="center" class="mb-4">
      <v-col cols="12" md="6">
        <h1 class="font-weight-bold mb-1 text-h4">Docker Manager</h1>
        <div class="d-flex align-center ga-2 flex-wrap text-body-2 text-medium-emphasis">
          <span>Visão geral do ambiente Docker</span>
          <template v-if="dfData">
            <span class="text-disabled">•</span>
            <v-chip color="primary" size="small" variant="tonal">
              Ocupado: <strong class="ml-1">{{ formatBytes(dfData.totalSize) }}</strong>
            </v-chip>
            <v-chip v-if="dfData.totalReclaimable > 0" color="warning" size="small" variant="tonal">
              Liberável: <strong class="ml-1">{{ formatBytes(dfData.totalReclaimable) }}</strong>
            </v-chip>
          </template>
        </div>
      </v-col>
      <v-col cols="12" md="6">
        <div class="d-flex align-center justify-start justify-md-end ga-2 flex-wrap">
          <v-btn
            color="warning"
            :disabled="pruneLoading"
            :loading="pruneLoading && !pruneAllSelected"
            prepend-icon="mdi-broom"
            variant="tonal"
            @click="openPruneDialog(false)"
          >
            Limpar Cache
          </v-btn>
          <v-btn
            color="error"
            :disabled="pruneLoading"
            :loading="pruneLoading && pruneAllSelected"
            prepend-icon="mdi-delete-sweep"
            variant="tonal"
            @click="openPruneDialog(true)"
          >
            Limpeza Completa
          </v-btn>
          <v-btn :loading="loading" prepend-icon="mdi-refresh" variant="tonal" @click="load">
            Atualizar
          </v-btn>
        </div>
      </v-col>
    </v-row>

    <DockerUnavailableBanner v-if="unavailable" />

    <template v-else>
      <!-- Unified Docker Overview Cards -->
      <v-row class="mb-4">
        <!-- Containers -->
        <v-col cols="12" md="3" sm="6">
          <v-card
            class="docker-overview-card h-100 cursor-pointer"
            to="/docker/containers"
            variant="outlined"
          >
            <v-card-text class="d-flex flex-column h-100 pa-5">
              <div class="d-flex align-center justify-space-between mb-3">
                <div class="d-flex align-center ga-3">
                  <v-avatar color="success" rounded="lg" size="44" variant="tonal">
                    <v-icon icon="mdi-cube-outline" size="24" />
                  </v-avatar>
                  <div>
                    <div class="text-subtitle-1 font-weight-bold line-height-1">Contêineres</div>
                    <div class="text-caption text-medium-emphasis">
                      {{ allContainers.length }} {{ allContainers.length === 1 ? 'total' : 'totais' }}
                    </div>
                  </div>
                </div>
                <v-icon class="card-arrow text-medium-emphasis" icon="mdi-arrow-right" size="18" />
              </div>

              <div class="d-flex align-baseline ga-2 mb-2">
                <span class="text-h5 font-weight-bold text-success">{{ running }}</span>
                <span class="text-caption text-medium-emphasis">em execução</span>
                <span class="text-caption text-medium-emphasis">•</span>
                <span
                  class="text-caption font-weight-medium"
                  :class="stopped > 0 ? 'text-error' : 'text-medium-emphasis'"
                >
                  {{ stopped }} {{ stopped === 1 ? 'parado' : 'parados' }}
                </span>
              </div>

              <v-divider class="my-2 border-opacity-25" />

              <div class="d-flex align-center justify-space-between text-caption mt-auto pt-1">
                <div class="d-flex align-center ga-1 text-medium-emphasis">
                  <v-icon icon="mdi-harddisk" size="14" />
                  <span>{{ formatBytes(dfData?.containers?.totalSize ?? 0) }} (RW)</span>
                </div>
                <v-chip
                  v-if="(dfData?.containers?.reclaimableSize ?? 0) > 0"
                  class="font-weight-medium"
                  color="warning"
                  size="x-small"
                  variant="tonal"
                >
                  {{ formatBytes(dfData!.containers.reclaimableSize) }} liberável
                </v-chip>
              </div>
            </v-card-text>
          </v-card>
        </v-col>

        <!-- Imagens -->
        <v-col cols="12" md="3" sm="6">
          <v-card
            class="docker-overview-card h-100 cursor-pointer"
            to="/docker/images"
            variant="outlined"
          >
            <v-card-text class="d-flex flex-column h-100 pa-5">
              <div class="d-flex align-center justify-space-between mb-3">
                <div class="d-flex align-center ga-3">
                  <v-avatar color="info" rounded="lg" size="44" variant="tonal">
                    <v-icon icon="mdi-layers-outline" size="24" />
                  </v-avatar>
                  <div>
                    <div class="text-subtitle-1 font-weight-bold line-height-1">Imagens</div>
                    <div class="text-caption text-medium-emphasis">
                      {{ dfData?.images?.totalCount ?? images }} {{ (dfData?.images?.totalCount ?? images) === 1 ? 'imagem' : 'imagens' }}
                    </div>
                  </div>
                </div>
                <v-icon class="card-arrow text-medium-emphasis" icon="mdi-arrow-right" size="18" />
              </div>

              <div class="d-flex align-baseline ga-2 mb-2">
                <span class="text-h5 font-weight-bold text-info">
                  {{ formatBytes(dfData?.images?.totalSize ?? 0) }}
                </span>
                <span class="text-caption text-medium-emphasis">em disco</span>
              </div>

              <v-divider class="my-2 border-opacity-25" />

              <div class="d-flex align-center justify-space-between text-caption mt-auto pt-1">
                <div class="text-medium-emphasis">
                  {{ dfData?.images?.activeCount ?? 0 }} ativas
                </div>
                <v-chip
                  v-if="(dfData?.images?.reclaimableSize ?? 0) > 0"
                  class="font-weight-medium"
                  color="warning"
                  size="x-small"
                  variant="tonal"
                >
                  {{ formatBytes(dfData!.images.reclaimableSize) }} liberável
                </v-chip>
              </div>
            </v-card-text>
          </v-card>
        </v-col>

        <!-- Volumes -->
        <v-col cols="12" md="3" sm="6">
          <v-card
            class="docker-overview-card h-100 cursor-pointer"
            to="/docker/volumes"
            variant="outlined"
          >
            <v-card-text class="d-flex flex-column h-100 pa-5">
              <div class="d-flex align-center justify-space-between mb-3">
                <div class="d-flex align-center ga-3">
                  <v-avatar color="primary" rounded="lg" size="44" variant="tonal">
                    <v-icon icon="mdi-database-outline" size="24" />
                  </v-avatar>
                  <div>
                    <div class="text-subtitle-1 font-weight-bold line-height-1">Volumes</div>
                    <div class="text-caption text-medium-emphasis">
                      {{ dfData?.volumes?.totalCount ?? volumes }} {{ (dfData?.volumes?.totalCount ?? volumes) === 1 ? 'volume' : 'volumes' }}
                    </div>
                  </div>
                </div>
                <v-icon class="card-arrow text-medium-emphasis" icon="mdi-arrow-right" size="18" />
              </div>

              <div class="d-flex align-baseline ga-2 mb-2">
                <span class="text-h5 font-weight-bold text-primary">
                  {{ formatBytes(dfData?.volumes?.totalSize ?? 0) }}
                </span>
                <span class="text-caption text-medium-emphasis">em disco</span>
              </div>

              <v-divider class="my-2 border-opacity-25" />

              <div class="d-flex align-center justify-space-between text-caption mt-auto pt-1">
                <div class="text-medium-emphasis">
                  {{ dfData?.volumes?.activeCount ?? 0 }} em uso
                </div>
                <v-chip
                  v-if="(dfData?.volumes?.reclaimableSize ?? 0) > 0"
                  class="font-weight-medium"
                  color="warning"
                  size="x-small"
                  variant="tonal"
                >
                  {{ formatBytes(dfData!.volumes.reclaimableSize) }} liberável
                </v-chip>
              </div>
            </v-card-text>
          </v-card>
        </v-col>

        <!-- Build Cache -->
        <v-col cols="12" md="3" sm="6">
          <v-card
            class="docker-overview-card h-100 cursor-pointer"
            variant="outlined"
            @click="openPruneDialog(false)"
          >
            <v-card-text class="d-flex flex-column h-100 pa-5">
              <div class="d-flex align-center justify-space-between mb-3">
                <div class="d-flex align-center ga-3">
                  <v-avatar color="secondary" rounded="lg" size="44" variant="tonal">
                    <v-icon icon="mdi-cached" size="24" />
                  </v-avatar>
                  <div>
                    <div class="text-subtitle-1 font-weight-bold line-height-1">Build Cache</div>
                    <div class="text-caption text-medium-emphasis">
                      {{ dfData?.buildCache?.totalCount ?? 0 }} {{ (dfData?.buildCache?.totalCount ?? 0) === 1 ? 'item' : 'itens' }}
                    </div>
                  </div>
                </div>
                <v-icon class="card-arrow text-medium-emphasis" icon="mdi-broom" size="18" />
              </div>

              <div class="d-flex align-baseline ga-2 mb-2">
                <span class="text-h5 font-weight-bold text-secondary">
                  {{ formatBytes(dfData?.buildCache?.totalSize ?? 0) }}
                </span>
                <span class="text-caption text-medium-emphasis">em disco</span>
              </div>

              <v-divider class="my-2 border-opacity-25" />

              <div class="d-flex align-center justify-space-between text-caption mt-auto pt-1">
                <div class="text-medium-emphasis">
                  {{ dfData?.buildCache?.activeCount ?? 0 }} ativos
                </div>
                <v-chip
                  v-if="(dfData?.buildCache?.reclaimableSize ?? 0) > 0"
                  class="font-weight-medium"
                  color="warning"
                  size="x-small"
                  variant="tonal"
                >
                  {{ formatBytes(dfData!.buildCache.reclaimableSize) }} liberável
                </v-chip>
                <span v-else class="text-caption text-secondary font-weight-medium">Limpar cache</span>
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

    <!-- Prune Confirmation Dialog -->
    <v-dialog v-model="pruneDialog" max-width="540">
      <v-card>
        <v-card-title class="d-flex align-center ga-2 pt-4 px-4">
          <v-avatar :color="pruneAllSelected ? 'error' : 'warning'" size="36">
            <v-icon :icon="pruneAllSelected ? 'mdi-delete-sweep' : 'mdi-broom'" />
          </v-avatar>
          <span class="text-h6 font-weight-bold">
            {{ pruneAllSelected ? 'Limpeza Completa (docker system prune -a)' : 'Limpar Cache do Docker' }}
          </span>
        </v-card-title>

        <v-card-text class="px-4 py-3">
          <p class="text-body-2 mb-3">
            {{
              pruneAllSelected
                ? 'Esta ação executará uma limpeza profunda equivalente a "docker system prune -a". Serão removidos todos os containers parados, redes não utilizadas, cache de build (BuildKit) e todas as imagens não associadas a um container em execução.'
                : 'Esta ação limpará o cache do Docker, removendo containers parados, redes não utilizadas, cache de build temporário e imagens dangling (camadas sem tag).'
            }}
          </p>

          <v-checkbox
            v-model="pruneIncludeVolumes"
            color="error"
            density="comfortable"
            hide-details
            label="Remover também volumes não utilizados (órfãos)"
          />
          <p v-if="pruneIncludeVolumes" class="text-caption text-error mt-1 ml-8">
            Atenção: volumes órfãos não associados a nenhum container serão excluídos permanentemente.
          </p>
        </v-card-text>

        <v-divider />

        <v-card-actions class="pa-3">
          <v-spacer />
          <v-btn
            :disabled="pruneLoading"
            variant="text"
            @click="pruneDialog = false"
          >
            Cancelar
          </v-btn>
          <v-btn
            :color="pruneAllSelected ? 'error' : 'warning'"
            :loading="pruneLoading"
            variant="flat"
            @click="executePrune"
          >
            Confirmar Limpeza
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

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
import { useNotifier } from '@/composables/useNotifier'
import { useResourceHistory } from '@/composables/useResourceHistory'
import { useSystemResources } from '@/composables/useSystemResources'
import { formatBytes } from '@/utils/format'
import type { DockerContainerGroup, DockerSystemDfResponse, SystemStatus } from '@/types/api'

const notify = useNotifier()
const loading = ref(false)
const unavailable = ref(false)
const groups = ref<DockerContainerGroup[]>([])
const volumes = ref(0)
const images = ref(0)
const dfData = ref<DockerSystemDfResponse | null>(null)

// Prune Dialog State
const pruneDialog = ref(false)
const pruneAllSelected = ref(false)
const pruneIncludeVolumes = ref(false)
const pruneLoading = ref(false)

function openPruneDialog(all: boolean) {
  pruneAllSelected.value = all
  pruneIncludeVolumes.value = false
  pruneDialog.value = true
}

async function executePrune() {
  pruneLoading.value = true
  try {
    const res = await dockerSystemApi.prune({
      all: pruneAllSelected.value,
      volumes: pruneIncludeVolumes.value,
    })
    const freed = formatBytes(res.spaceReclaimed)
    const counts: string[] = []
    if (res.containersDeleted && res.containersDeleted.length > 0) {
      counts.push(`${res.containersDeleted.length} container(s)`)
    }
    if (res.imagesDeleted && res.imagesDeleted.length > 0) {
      counts.push(`${res.imagesDeleted.length} imagem(ns)`)
    }
    if (res.networksDeleted && res.networksDeleted.length > 0) {
      counts.push(`${res.networksDeleted.length} rede(s)`)
    }
    if (res.volumesDeleted && res.volumesDeleted.length > 0) {
      counts.push(`${res.volumesDeleted.length} volume(s)`)
    }
    if (res.buildCacheDeleted && res.buildCacheDeleted.length > 0) {
      counts.push(`${res.buildCacheDeleted.length} cache(s) de build`)
    }

    const detailMsg = counts.length > 0 ? ` (${counts.join(', ')})` : ''
    notify(`Limpeza concluída com sucesso! ${freed} liberados${detailMsg}.`, 'success')
    pruneDialog.value = false
    await load()
  } catch (error) {
    notify(error instanceof Error ? error.message : 'Erro ao executar limpeza.', 'error')
  } finally {
    pruneLoading.value = false
  }
}

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
.docker-overview-card {
  border: 1px solid rgba(var(--v-border-color), 0.12);
  transition: transform 0.2s ease, box-shadow 0.2s ease, border-color 0.2s ease;
  position: relative;
  overflow: hidden;
}
.docker-overview-card:hover {
  transform: translateY(-3px);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.08);
  border-color: rgba(var(--v-theme-primary), 0.4);
}
.docker-overview-card:hover .card-arrow {
  transform: translateX(3px);
  color: rgb(var(--v-theme-primary)) !important;
}
.card-arrow {
  transition: transform 0.2s ease, color 0.2s ease;
}
.line-height-1 {
  line-height: 1.2;
}
</style>
