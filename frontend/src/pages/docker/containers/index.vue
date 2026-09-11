<template>
  <div>
    <v-row align="center" class="mb-4">
      <v-col>
        <v-breadcrumbs :items="['Docker', 'Containers']" class="pa-0" />
        <h1 class="font-weight-bold text-h5 mt-1">Containers</h1>
      </v-col>
      <v-col cols="auto">
        <div class="d-flex align-center ga-2">
          <v-chip-group v-model="stateFilter" selected-class="text-primary" variant="outlined">
            <v-chip value="all">Todos</v-chip>
            <v-chip value="running">Em execução</v-chip>
            <v-chip value="stopped">Parados</v-chip>
          </v-chip-group>
          <v-btn
            :color="autoRefresh ? 'success' : undefined"
            :prepend-icon="autoRefresh ? 'mdi-sync' : 'mdi-sync-off'"
            :title="autoRefresh ? 'Auto-refresh ativo (30s)' : 'Auto-refresh desativado'"
            :variant="autoRefresh ? 'tonal' : 'outlined'"
            @click="toggleAutoRefresh"
          >
            {{ autoRefresh ? '30s' : 'Auto' }}
          </v-btn>
          <v-btn :loading="loading" prepend-icon="mdi-refresh" variant="tonal" @click="load">
            Atualizar
          </v-btn>
          <v-btn
            color="warning"
            :disabled="actionLoading || pruneLoading"
            :loading="pruneLoading && !pruneAllSelected"
            prepend-icon="mdi-broom"
            variant="tonal"
            @click="openPruneDialog(false)"
          >
            Limpar Cache
          </v-btn>
          <v-btn
            color="error"
            :disabled="actionLoading || pruneLoading"
            :loading="pruneLoading && pruneAllSelected"
            prepend-icon="mdi-delete-sweep"
            variant="tonal"
            @click="openPruneDialog(true)"
          >
            Limpeza Completa
          </v-btn>
        </div>
      </v-col>
    </v-row>

    <DockerUnavailableBanner v-if="unavailable" />

    <v-progress-linear v-else-if="loading" indeterminate />

    <template v-else>
      <ContainerProjectGroup
        v-for="group in filteredGroups"
        :key="group.projectName"
        :group="group"
        :loading="actionLoading"
        :resources-by-id="resourcesByContainerId"
        @clear-group-logs="handleClearGroupLogs"
        @download-group-logs="handleDownloadGroupLogs"
        @restart="handleAction('restart', $event)"
        @restart-all="handleAll('restart', $event)"
        @start="handleAction('start', $event)"
        @stop="handleAction('stop', $event)"
        @stop-all="handleAll('stop', $event)"
      />

      <v-alert v-if="filteredGroups.length === 0" border="start" type="info" variant="tonal">
        Nenhum container encontrado para o filtro selecionado.
      </v-alert>
    </template>

    <DockerActionConfirmDialog
      v-model="confirmDialog"
      :loading="actionLoading"
      :message="confirmMessage"
      @cancel="confirmDialog = false"
      @confirm="executeConfirmed"
    />

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
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { DockerContainerGroup, DockerContainerResourceMetrics } from '@/types/api'
import { dockerContainersApi, dockerSystemApi } from '@/services/dockerService'
import { useDockerContainerResources } from '@/composables/useDockerContainerResources'
import { useNotifier } from '@/composables/useNotifier'
import { formatBytes } from '@/utils/format'
import ContainerProjectGroup from '@/components/docker/ContainerProjectGroup.vue'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import DockerActionConfirmDialog from '@/components/docker/DockerActionConfirmDialog.vue'

type StateFilter = 'all' | 'running' | 'stopped'
type ActionType = 'start' | 'stop' | 'restart'

const notify = useNotifier()
const groups = ref<DockerContainerGroup[]>([])
const loading = ref(false)
const actionLoading = ref(false)
const unavailable = ref(false)
const stateFilter = ref<StateFilter>('all')
const autoRefresh = ref(false)
let refreshTimer: ReturnType<typeof setInterval> | null = null

const { overview: resourcesOverview } = useDockerContainerResources()

const resourcesByContainerId = computed((): Record<string, DockerContainerResourceMetrics> => {
  const map: Record<string, DockerContainerResourceMetrics> = {}
  for (const c of resourcesOverview.value?.containers ?? []) {
    map[c.containerId] = c
  }
  return map
})

const REFRESH_INTERVAL_MS = 30_000

const confirmDialog = ref(false)
const confirmMessage = ref('')
let pendingAction: (() => Promise<void>) | null = null

const filteredGroups = computed((): DockerContainerGroup[] => {
  if (stateFilter.value === 'all') return groups.value
  return groups.value
    .map((g) => ({
      ...g,
      containers: g.containers.filter((c) =>
        stateFilter.value === 'running' ? c.state === 'running' : c.state !== 'running'
      ),
    }))
    .filter((g) => g.containers.length > 0)
})

async function load() {
  loading.value = true
  unavailable.value = false
  try {
    groups.value = await dockerContainersApi.getGroups()
  } catch {
    unavailable.value = true
  } finally {
    loading.value = false
  }
}

function toggleAutoRefresh() {
  autoRefresh.value = !autoRefresh.value
  if (autoRefresh.value) {
    refreshTimer = setInterval(load, REFRESH_INTERVAL_MS)
  } else {
    if (refreshTimer !== null) {
      clearInterval(refreshTimer)
      refreshTimer = null
    }
  }
}

function handleAction(action: ActionType, id: string) {
  const labels: Record<ActionType, string> = {
    start: 'iniciar',
    stop: 'parar',
    restart: 'reiniciar',
  }
  confirmMessage.value = `Deseja ${labels[action]} o container?`
  pendingAction = () => dockerContainersApi[action](id).then(() => load())
  confirmDialog.value = true
}

function handleAll(action: 'stop' | 'restart', ids: string[]) {
  const labels = { stop: 'parar', restart: 'reiniciar' }
  confirmMessage.value = `Deseja ${labels[action]} todos os ${ids.length} containers deste projeto?`
  pendingAction = () => Promise.all(ids.map((id) => dockerContainersApi[action](id))).then(() => load())
  confirmDialog.value = true
}

function handleClearGroupLogs(group: DockerContainerGroup) {
  const name = group.projectName === '_standalone' ? 'Standalone' : group.projectName
  const ids = group.containers.map((c) => c.id)
  confirmMessage.value = `Deseja realmente limpar todos os logs dos ${ids.length} containers do grupo "${name}"? Esta ação não pode ser desfeita.`
  pendingAction = async () => {
    await dockerContainersApi.clearGroupLogs(ids)
    notify(`Logs do grupo "${name}" limpos com sucesso.`, 'success')
  }
  confirmDialog.value = true
}

async function handleDownloadGroupLogs(group: DockerContainerGroup) {
  const name = group.projectName === '_standalone' ? 'Standalone' : group.projectName
  const containers = group.containers.map((c) => ({
    id: c.id,
    name: c.names[0] || c.id.slice(0, 12),
  }))
  notify(`Iniciando o download dos logs do grupo "${name}"...`, 'info')
  try {
    await dockerContainersApi.downloadGroupLogs(group.projectName, containers)
  } catch (error) {
    notify(error instanceof Error ? error.message : 'Erro ao baixar logs do grupo.', 'error')
  }
}

async function executeConfirmed() {
  if (!pendingAction) return
  actionLoading.value = true
  try {
    await pendingAction()
  } finally {
    actionLoading.value = false
    confirmDialog.value = false
    pendingAction = null
  }
}

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

onMounted(load)

onUnmounted(() => {
  if (refreshTimer !== null) clearInterval(refreshTimer)
})
</script>
