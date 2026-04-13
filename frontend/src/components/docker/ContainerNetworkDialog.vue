<template>
  <v-dialog v-model="model" max-width="560" scrollable>
    <v-card>
      <v-card-title class="d-flex align-center pa-4">
        <v-icon class="mr-2" icon="mdi-lan-connect" />
        Gerenciar Redes — {{ containerName }}
      </v-card-title>
      <v-divider />

      <v-card-text class="pa-4" style="max-height: 500px">
        <!-- Redes atuais -->
        <div class="text-subtitle-2 mb-2">Redes atuais</div>
        <v-list v-if="currentNetworks.length > 0" density="compact" class="mb-4">
          <v-list-item
            v-for="net in currentNetworks"
            :key="net.networkId"
            :subtitle="net.networkId.slice(0, 12)"
            :title="net.networkName"
          >
            <template #append>
              <v-btn
                color="error"
                density="compact"
                :disabled="loading || currentNetworks.length <= 1"
                icon="mdi-minus-circle-outline"
                size="small"
                :title="currentNetworks.length <= 1 ? 'O container precisa estar em ao menos uma rede' : `Desconectar de ${net.networkName}`"
                variant="text"
                @click="disconnect(net.networkId, net.networkName)"
              />
            </template>
          </v-list-item>
        </v-list>
        <v-alert v-else density="compact" type="warning" variant="tonal" class="mb-4">
          Container não está em nenhuma rede.
        </v-alert>

        <v-divider class="mb-4" />

        <!-- Conectar a rede existente -->
        <div class="text-subtitle-2 mb-3">Conectar a uma rede existente</div>
        <v-row dense>
          <v-col cols="12">
            <v-select
              v-model="selectedNetwork"
              clearable
              density="compact"
              hide-details
              item-title="name"
              item-value="id"
              :items="availableNetworks"
              label="Selecionar rede"
              :loading="loadingNetworks"
              no-data-text="Nenhuma rede disponível"
              prepend-inner-icon="mdi-lan"
              variant="outlined"
            />
          </v-col>
          <v-col cols="12">
            <v-btn
              block
              color="primary"
              :disabled="!selectedNetwork || loading"
              :loading="loading"
              prepend-icon="mdi-lan-connect"
              variant="tonal"
              @click="connectExisting"
            >
              Conectar
            </v-btn>
          </v-col>
        </v-row>

        <v-divider class="my-4" />

        <!-- Criar nova rede e conectar -->
        <div class="text-subtitle-2 mb-3">Criar nova rede e conectar</div>
        <v-row dense>
          <v-col cols="12" sm="8">
            <v-text-field
              v-model="newNetworkName"
              density="compact"
              hide-details
              label="Nome da nova rede"
              prepend-inner-icon="mdi-plus-network-outline"
              variant="outlined"
            />
          </v-col>
          <v-col cols="12" sm="4">
            <v-select
              v-model="newNetworkDriver"
              density="compact"
              hide-details
              :items="['bridge', 'overlay', 'host', 'none']"
              label="Driver"
              variant="outlined"
            />
          </v-col>
          <v-col cols="12">
            <v-btn
              block
              color="success"
              :disabled="!newNetworkName.trim() || loading"
              :loading="loading"
              prepend-icon="mdi-plus-network"
              variant="tonal"
              @click="createAndConnect"
            >
              Criar e Conectar
            </v-btn>
          </v-col>
        </v-row>

        <v-alert v-if="errorMsg" class="mt-3" closable color="error" density="compact" variant="tonal" @click:close="errorMsg = ''">
          {{ errorMsg }}
        </v-alert>
      </v-card-text>

      <v-divider />
      <v-card-actions class="justify-end pa-3">
        <v-btn variant="text" @click="model = false">Fechar</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import { ref, watch } from 'vue'
import type { DockerNetworkEndpoint, DockerNetworkSummary } from '@/types/api'
import { dockerNetworksApi } from '@/services/dockerService'

const model = defineModel<boolean>({ default: false })

const props = defineProps<{
  containerId: string
  containerName: string
  currentNetworks: DockerNetworkEndpoint[]
}>()

const emit = defineEmits<{
  (e: 'updated'): void
}>()

const loading = ref(false)
const loadingNetworks = ref(false)
const allNetworks = ref<DockerNetworkSummary[]>([])
const selectedNetwork = ref<string | null>(null)
const newNetworkName = ref('')
const newNetworkDriver = ref('bridge')
const errorMsg = ref('')

const availableNetworks = ref<DockerNetworkSummary[]>([])

async function loadNetworks() {
  loadingNetworks.value = true
  try {
    allNetworks.value = await dockerNetworksApi.list()
    refreshAvailable()
  } catch {
    // silently ignore
  } finally {
    loadingNetworks.value = false
  }
}

function refreshAvailable() {
  const currentIds = new Set(props.currentNetworks.map((n) => n.networkId))
  availableNetworks.value = allNetworks.value.filter((n) => !currentIds.has(n.id))
}

async function connectExisting() {
  if (!selectedNetwork.value) return
  loading.value = true
  errorMsg.value = ''
  try {
    await dockerNetworksApi.connect(selectedNetwork.value, props.containerId)
    selectedNetwork.value = null
    emit('updated')
  } catch (error) {
    errorMsg.value = error instanceof Error ? error.message : 'Falha ao conectar à rede.'
  } finally {
    loading.value = false
  }
}

async function createAndConnect() {
  const name = newNetworkName.value.trim()
  if (!name) return
  loading.value = true
  errorMsg.value = ''
  try {
    await dockerNetworksApi.create(name, newNetworkDriver.value)
    // Recarrega lista de redes para obter o ID da nova rede
    const networks = await dockerNetworksApi.list()
    const created = networks.find((n) => n.name === name)
    if (created) {
      await dockerNetworksApi.connect(created.id, props.containerId)
    }
    newNetworkName.value = ''
    emit('updated')
  } catch (error) {
    errorMsg.value = error instanceof Error ? error.message : 'Falha ao criar rede.'
  } finally {
    loading.value = false
  }
}

async function disconnect(networkId: string, networkName: string) {
  loading.value = true
  errorMsg.value = ''
  try {
    await dockerNetworksApi.disconnect(networkId, props.containerId)
    emit('updated')
  } catch (error) {
    errorMsg.value =
      error instanceof Error ? error.message : `Falha ao desconectar de "${networkName}".`
  } finally {
    loading.value = false
  }
}

watch(model, (opened) => {
  if (opened) {
    errorMsg.value = ''
    selectedNetwork.value = null
    newNetworkName.value = ''
    newNetworkDriver.value = 'bridge'
    void loadNetworks()
  }
})

watch(() => props.currentNetworks, refreshAvailable, { deep: true })
</script>
