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
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { DockerContainerGroup, DockerContainerResourceMetrics } from '@/types/api'
import { dockerContainersApi } from '@/services/dockerService'
import { useDockerContainerResources } from '@/composables/useDockerContainerResources'
import ContainerProjectGroup from '@/components/docker/ContainerProjectGroup.vue'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import DockerActionConfirmDialog from '@/components/docker/DockerActionConfirmDialog.vue'

type StateFilter = 'all' | 'running' | 'stopped'
type ActionType = 'start' | 'stop' | 'restart'

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

onMounted(load)

onUnmounted(() => {
  if (refreshTimer !== null) clearInterval(refreshTimer)
})
</script>
