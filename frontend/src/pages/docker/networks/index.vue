<template>
  <div>
    <v-row align="center" class="mb-4">
      <v-col>
        <v-breadcrumbs :items="['Docker', 'Redes']" class="pa-0" />
        <h1 class="font-weight-bold text-h5 mt-1">Redes</h1>
      </v-col>
      <v-col cols="auto">
        <div class="d-flex ga-2">
          <v-btn
            color="primary"
            :disabled="unavailable"
            prepend-icon="mdi-plus-network-outline"
            variant="elevated"
            @click="openCreateDialog"
          >
            Criar rede
          </v-btn>
          <v-btn :loading="loading" prepend-icon="mdi-refresh" variant="tonal" @click="load">
            Atualizar
          </v-btn>
        </div>
      </v-col>
    </v-row>

    <DockerUnavailableBanner v-if="unavailable" />

    <template v-else>
      <v-text-field
        v-model="search"
        class="mb-4"
        clearable
        density="compact"
        hide-details
        placeholder="Buscar rede..."
        prepend-inner-icon="mdi-magnify"
        style="max-width: 400px"
        variant="outlined"
      />

      <v-progress-linear v-if="loading" indeterminate />

      <v-row v-else dense>
        <v-col
          v-for="net in filtered"
          :key="net.id"
          cols="12"
          md="4"
          sm="6"
        >
          <NetworkCard
            :loading="actionLoading"
            :network="net"
            @detail="showDetail"
            @remove="requestRemove"
          />
        </v-col>
        <v-col v-if="filtered.length === 0" cols="12">
          <v-alert border="start" type="info" variant="tonal">Nenhuma rede encontrada.</v-alert>
        </v-col>
      </v-row>
    </template>

    <NetworkDetailDialog v-model="detailDialog" :detail="selectedDetail" />

    <DockerActionConfirmDialog
      v-model="confirmDialog"
      :loading="actionLoading"
      :message="confirmMessage"
      confirm-label="Remover"
      @cancel="confirmDialog = false"
      @confirm="executeRemove"
    />

    <v-dialog v-model="createDialog" max-width="500" persistent>
      <v-form @submit.prevent="executeCreate">
        <v-card>
          <v-card-title class="d-flex align-center pa-4">
            <v-icon class="mr-2" color="primary" icon="mdi-plus-network-outline" />
            Criar rede Docker
          </v-card-title>
          <v-divider />
          <v-card-text class="pa-4">
            <v-text-field
              v-model="newNetworkName"
              autofocus
              label="Nome da rede"
              placeholder="minha-rede"
              prepend-inner-icon="mdi-lan"
              variant="outlined"
            />
            <v-select
              v-model="newNetworkDriver"
              hide-details
              :items="networkDrivers"
              label="Driver"
              variant="outlined"
            />
          </v-card-text>
          <v-divider />
          <v-card-actions class="justify-end pa-3 ga-2">
            <v-btn :disabled="actionLoading" variant="text" @click="createDialog = false">
              Cancelar
            </v-btn>
            <v-btn
              color="primary"
              :disabled="!newNetworkName.trim()"
              :loading="actionLoading"
              prepend-icon="mdi-plus"
              type="submit"
              variant="elevated"
            >
              Criar
            </v-btn>
          </v-card-actions>
        </v-card>
      </v-form>
    </v-dialog>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import type { DockerNetworkDetail, DockerNetworkSummary } from '@/types/api'
import { dockerNetworksApi } from '@/services/dockerService'
import { useNotifier } from '@/composables/useNotifier'
import NetworkCard from '@/components/docker/NetworkCard.vue'
import NetworkDetailDialog from '@/components/docker/NetworkDetailDialog.vue'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import DockerActionConfirmDialog from '@/components/docker/DockerActionConfirmDialog.vue'

const networks = ref<DockerNetworkSummary[]>([])
const loading = ref(false)
const actionLoading = ref(false)
const unavailable = ref(false)
const search = ref('')
const detailDialog = ref(false)
const createDialog = ref(false)
const confirmDialog = ref(false)
const confirmMessage = ref('')
const selectedDetail = ref<DockerNetworkDetail | null>(null)
const newNetworkName = ref('')
const newNetworkDriver = ref('bridge')
const networkDrivers = ['bridge', 'overlay', 'macvlan', 'ipvlan']
let pendingRemove: DockerNetworkSummary | null = null

const notify = useNotifier()

const filtered = computed(() => {
  const query = search.value.trim().toLowerCase()
  if (!query) return networks.value
  return networks.value.filter((network) =>
    network.name.toLowerCase().includes(query)
      || network.driver.toLowerCase().includes(query)
  )
})

async function load() {
  loading.value = true
  unavailable.value = false
  try {
    networks.value = await dockerNetworksApi.list()
  } catch {
    unavailable.value = true
  } finally {
    loading.value = false
  }
}

async function showDetail(net: DockerNetworkSummary) {
  try {
    selectedDetail.value = await dockerNetworksApi.getDetail(net.id)
    detailDialog.value = true
  } catch {
    selectedDetail.value = { ...net, containers: {}, options: {} }
    detailDialog.value = true
  }
}

function openCreateDialog() {
  newNetworkName.value = ''
  newNetworkDriver.value = 'bridge'
  createDialog.value = true
}

async function executeCreate() {
  const name = newNetworkName.value.trim()
  if (!name) return

  actionLoading.value = true
  try {
    const result = await dockerNetworksApi.create(name, newNetworkDriver.value)
    createDialog.value = false
    notify(result.message || `Rede "${name}" criada com sucesso.`, 'success')
    await load()
  } catch (error) {
    notify(error instanceof Error ? error.message : 'Erro ao criar rede.', 'error')
  } finally {
    actionLoading.value = false
  }
}

function requestRemove(network: DockerNetworkSummary) {
  if ((network.runningContainers ?? network.connectedContainers) > 0) return

  pendingRemove = network
  const stoppedConnections = network.connectedContainers - (network.runningContainers ?? 0)
  const stoppedNotice = stoppedConnections > 0
    ? ` ${stoppedConnections} contêiner(es) parado(s) serão desconectados.`
    : ''
  confirmMessage.value = `Deseja remover a rede "${network.name}"?${stoppedNotice} Esta ação não pode ser desfeita.`
  confirmDialog.value = true
}

async function executeRemove() {
  if (!pendingRemove) return

  actionLoading.value = true
  try {
    const result = await dockerNetworksApi.remove(pendingRemove.id)
    notify(result.message || `Rede "${pendingRemove.name}" removida com sucesso.`, 'success')
    confirmDialog.value = false
    pendingRemove = null
    await load()
  } catch (error) {
    notify(error instanceof Error ? error.message : 'Erro ao remover rede.', 'error')
  } finally {
    actionLoading.value = false
  }
}

onMounted(load)
</script>
