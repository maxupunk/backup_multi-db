<template>
  <div>
    <v-row align="center" class="mb-4">
      <v-col>
        <v-breadcrumbs :items="['Docker', 'Redes']" class="pa-0" />
        <h1 class="font-weight-bold text-h5 mt-1">Redes</h1>
      </v-col>
      <v-col cols="auto">
        <v-btn :loading="loading" prepend-icon="mdi-refresh" variant="tonal" @click="load">
          Atualizar
        </v-btn>
      </v-col>
    </v-row>

    <DockerUnavailableBanner v-if="unavailable" />

    <v-progress-linear v-else-if="loading" indeterminate />

    <v-row v-else dense>
      <v-col
        v-for="net in networks"
        :key="net.id"
        cols="12"
        md="4"
        sm="6"
      >
        <NetworkCard :network="net" @detail="showDetail" />
      </v-col>
      <v-col v-if="networks.length === 0" cols="12">
        <v-alert border="start" type="info" variant="tonal">Nenhuma rede encontrada.</v-alert>
      </v-col>
    </v-row>

    <NetworkDetailDialog v-model="detailDialog" :detail="selectedDetail" />
  </div>
</template>

<script lang="ts" setup>
import { onMounted, ref } from 'vue'
import type { DockerNetworkDetail, DockerNetworkSummary } from '@/types/api'
import { dockerNetworksApi } from '@/services/dockerService'
import NetworkCard from '@/components/docker/NetworkCard.vue'
import NetworkDetailDialog from '@/components/docker/NetworkDetailDialog.vue'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'

const networks = ref<DockerNetworkSummary[]>([])
const loading = ref(false)
const unavailable = ref(false)
const detailDialog = ref(false)
const selectedDetail = ref<DockerNetworkDetail | null>(null)

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

onMounted(load)
</script>
