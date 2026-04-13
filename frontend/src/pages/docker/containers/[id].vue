<template>
  <div>
    <v-breadcrumbs
      class="pa-0 mb-2"
      :items="[
        { title: 'Docker', to: '/docker' },
        { title: 'Containers', to: '/docker/containers' },
        { title: containerName },
      ]"
    />

    <v-progress-linear v-if="loading" indeterminate />

    <DockerUnavailableBanner v-else-if="error" :reason="error" />

    <template v-else-if="detail">
      <!-- Header -->
      <div class="d-flex align-center flex-wrap ga-3 mb-4">
        <div>
          <h1 class="font-weight-bold text-h5">{{ containerName }}</h1>
          <span class="text-caption text-medium-emphasis">{{ detail.imageId.slice(0, 20) }}</span>
        </div>
        <ContainerStatusChip :state="detail.state.status" />
        <v-spacer />
        <div class="d-flex ga-2 flex-wrap">
          <v-btn
            color="success"
            density="comfortable"
            :disabled="detail.state.running || actionLoading"
            prepend-icon="mdi-play"
            variant="tonal"
            @click="handleAction('start')"
          >
            Iniciar
          </v-btn>
          <v-btn
            color="error"
            density="comfortable"
            :disabled="!detail.state.running || actionLoading"
            prepend-icon="mdi-stop"
            variant="tonal"
            @click="handleAction('stop')"
          >
            Parar
          </v-btn>
          <v-btn
            color="warning"
            density="comfortable"
            :disabled="actionLoading"
            prepend-icon="mdi-restart"
            variant="tonal"
            @click="handleAction('restart')"
          >
            Reiniciar
          </v-btn>
          <v-btn
            color="error"
            density="comfortable"
            :disabled="actionLoading"
            prepend-icon="mdi-delete"
            variant="outlined"
            @click="removeDialog = true"
          >
            Remover
          </v-btn>
        </div>
      </div>

      <!-- Tabs -->
      <v-card>
        <v-tabs v-model="tab">
          <v-tab value="info">Informações</v-tab>
          <v-tab value="env">Ambiente</v-tab>
          <v-tab value="volumes">Volumes</v-tab>
          <v-tab value="networks">Redes</v-tab>
          <v-tab value="ports">Portas</v-tab>
          <v-tab value="logs">Logs</v-tab>
        </v-tabs>
        <v-divider />

        <v-tabs-window v-model="tab">
          <!-- Informações -->
          <v-tabs-window-item value="info">
            <v-card-text>
              <v-list density="compact" lines="two">
                <v-list-item subtitle="ID" :title="detail.id" />
                <v-list-item subtitle="Imagem" :title="detail.imageId" />
                <v-list-item subtitle="Criado em" :title="detail.created" />
                <v-list-item subtitle="Status" :title="detail.state.status" />
                <v-list-item subtitle="PID" :title="String(detail.state.pid)" />
                <v-list-item subtitle="Iniciado em" :title="detail.state.startedAt" />
                <v-list-item
                  v-if="!detail.state.running"
                  subtitle="Finalizado em"
                  :title="detail.state.finishedAt"
                />
                <v-list-item
                  subtitle="Restart Policy"
                  :title="detail.hostConfig.restartPolicy.name"
                />
                <v-list-item
                  v-if="detail.config.cmd"
                  subtitle="CMD"
                  :title="detail.config.cmd.join(' ')"
                />
                <v-list-item
                  v-if="detail.config.entrypoint"
                  subtitle="Entrypoint"
                  :title="detail.config.entrypoint.join(' ')"
                />
                <v-list-item
                  v-if="detail.config.workingDir"
                  subtitle="Working Dir"
                  :title="detail.config.workingDir"
                />
              </v-list>
            </v-card-text>
          </v-tabs-window-item>

          <!-- Ambiente -->
          <v-tabs-window-item value="env">
            <v-card-text>
              <ContainerEnvironmentTable :env="detail.config.env" />
            </v-card-text>
          </v-tabs-window-item>

          <!-- Volumes -->
          <v-tabs-window-item value="volumes">
            <v-card-text>
              <ContainerMountsTable :mounts="detail.mounts" />
              <template v-if="namedVolumes.length > 0">
                <v-divider class="my-4" />
                <p class="text-caption font-weight-bold mb-3">Exportar volume como arquivo</p>
                <div class="d-flex flex-wrap ga-2">
                  <v-btn
                    v-for="mount in namedVolumes"
                    :key="mount.name"
                    color="primary"
                    density="comfortable"
                    prepend-icon="mdi-archive-arrow-down-outline"
                    size="small"
                    variant="tonal"
                    @click="exportVolume(mount.name!)"
                  >
                    {{ mount.name }}
                  </v-btn>
                </div>
              </template>
            </v-card-text>
          </v-tabs-window-item>

          <!-- Redes -->
          <v-tabs-window-item value="networks">
            <v-card-text>
              <div class="d-flex justify-end mb-3">
                <v-btn
                  color="primary"
                  density="comfortable"
                  prepend-icon="mdi-lan-connect"
                  variant="tonal"
                  @click="networkDialog = true"
                >
                  Gerenciar Redes
                </v-btn>
              </div>
              <ContainerNetworkTable :networks="detail.networks" />
            </v-card-text>
          </v-tabs-window-item>

          <!-- Portas -->
          <v-tabs-window-item value="ports">
            <v-card-text>
              <ContainerPortsTable :ports="detail.config.labels ? [] : []" />
              <p class="text-caption text-medium-emphasis">
                Consulte as portas na aba Informações ou via inspeção Docker.
              </p>
            </v-card-text>
          </v-tabs-window-item>

          <!-- Logs -->
          <v-tabs-window-item value="logs">
            <v-card-text>
              <ContainerLogsViewer :container-id="getContainerId()" />
            </v-card-text>
          </v-tabs-window-item>
        </v-tabs-window>
      </v-card>
    </template>

    <DockerActionConfirmDialog
      v-model="confirmDialog"
      :loading="actionLoading"
      :message="confirmMessage"
      @cancel="confirmDialog = false"
      @confirm="executeConfirmed"
    />

    <ContainerNetworkDialog
      v-if="detail"
      v-model="networkDialog"
      :container-id="getContainerId()"
      :container-name="containerName"
      :current-networks="detail.networks"
      @updated="load"
    />

    <ContainerRemoveDialog
      v-model="removeDialog"
      :container-name="containerName"
      :loading="removeLoading"
      @cancel="removeDialog = false"
      @confirm="executeRemove"
    />
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import type { DockerContainerDetail } from '@/types/api'
import { dockerContainersApi, dockerVolumesApi } from '@/services/dockerService'
import { useNotifier } from '@/composables/useNotifier'
import ContainerStatusChip from '@/components/docker/ContainerStatusChip.vue'
import ContainerEnvironmentTable from '@/components/docker/ContainerEnvironmentTable.vue'
import ContainerMountsTable from '@/components/docker/ContainerMountsTable.vue'
import ContainerNetworkTable from '@/components/docker/ContainerNetworkTable.vue'
import ContainerPortsTable from '@/components/docker/ContainerPortsTable.vue'
import ContainerLogsViewer from '@/components/docker/ContainerLogsViewer.vue'
import ContainerNetworkDialog from '@/components/docker/ContainerNetworkDialog.vue'
import ContainerRemoveDialog from '@/components/docker/ContainerRemoveDialog.vue'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import DockerActionConfirmDialog from '@/components/docker/DockerActionConfirmDialog.vue'

type ActionType = 'start' | 'stop' | 'restart'

const route = useRoute()
const router = useRouter()
const notify = useNotifier()

function getContainerId(): string {
  const params = route.params as Record<string, string | string[]>
  const id = params['id']
  return Array.isArray(id) ? (id[0] ?? '') : (id ?? '')
}
const detail = ref<DockerContainerDetail | null>(null)
const loading = ref(false)
const actionLoading = ref(false)
const removeLoading = ref(false)
const error = ref<string | null>(null)
const tab = ref('info')

const confirmDialog = ref(false)
const confirmMessage = ref('')
const networkDialog = ref(false)
const removeDialog = ref(false)
let pendingAction: (() => Promise<void>) | null = null

const containerName = computed(() => detail.value?.name ?? getContainerId().slice(0, 12))
const namedVolumes = computed(() =>
  (detail.value?.mounts ?? []).filter((m) => m.type === 'volume' && !!m.name)
)

async function load() {
  loading.value = true
  error.value = null
  try {
    detail.value = await dockerContainersApi.getDetail(getContainerId())
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Erro ao carregar container.'
  } finally {
    loading.value = false
  }
}

function handleAction(action: ActionType) {
  const labels: Record<ActionType, string> = { start: 'iniciar', stop: 'parar', restart: 'reiniciar' }
  confirmMessage.value = `Deseja ${labels[action]} o container "${containerName.value}"?`
  pendingAction = async () => {
    await dockerContainersApi[action](getContainerId())
    await load()
  }
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

function exportVolume(name: string) {
  notify(`Iniciando exportação do volume "${name}"...`, 'info')
  dockerVolumesApi.exportVolume(name)
}

async function executeRemove(force: boolean) {
  removeLoading.value = true
  try {
    await dockerContainersApi.remove(getContainerId(), force)
    notify(`Container "${containerName.value}" removido com sucesso.`, 'success')
    await router.push('/docker/containers')
  } catch (e) {
    notify(e instanceof Error ? e.message : 'Erro ao remover container.', 'error')
  } finally {
    removeLoading.value = false
    removeDialog.value = false
  }
}

onMounted(load)
</script>
