<template>
  <div>
    <v-row align="center" class="mb-4">
      <v-col>
        <v-breadcrumbs :items="['Docker', 'Volumes']" class="pa-0" />
        <h1 class="font-weight-bold text-h5 mt-1">Volumes</h1>
      </v-col>
      <v-col cols="auto">
        <v-btn :loading="loading" prepend-icon="mdi-refresh" variant="tonal" @click="load">
          Atualizar
        </v-btn>
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
        placeholder="Buscar volume..."
        prepend-inner-icon="mdi-magnify"
        style="max-width: 400px"
        variant="outlined"
      />

      <v-progress-linear v-if="loading" indeterminate />

      <v-row v-else dense>
        <v-col
          v-for="vol in filtered"
          :key="vol.name"
          cols="12"
          md="4"
          sm="6"
        >
          <VolumeCard
            :loading="actionLoading"
            :volume="vol"
            @detail="showDetail"
            @remove="requestRemove"
          />
        </v-col>
        <v-col v-if="filtered.length === 0" cols="12">
          <v-alert border="start" type="info" variant="tonal">Nenhum volume encontrado.</v-alert>
        </v-col>
      </v-row>
    </template>

    <VolumeDetailDialog v-model="detailDialog" :detail="selectedDetail" />

    <DockerActionConfirmDialog
      v-model="confirmDialog"
      :loading="actionLoading"
      message="Deseja remover este volume? Esta ação não pode ser desfeita."
      @cancel="confirmDialog = false"
      @confirm="executeRemove"
    />
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import type { DockerVolumeSummary, DockerVolumeDetail } from '@/types/api'
import { dockerVolumesApi } from '@/services/dockerService'
import VolumeCard from '@/components/docker/VolumeCard.vue'
import VolumeDetailDialog from '@/components/docker/VolumeDetailDialog.vue'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import DockerActionConfirmDialog from '@/components/docker/DockerActionConfirmDialog.vue'

const volumes = ref<DockerVolumeSummary[]>([])
const loading = ref(false)
const actionLoading = ref(false)
const unavailable = ref(false)
const search = ref('')
const detailDialog = ref(false)
const confirmDialog = ref(false)
const selectedDetail = ref<DockerVolumeDetail | null>(null)
let pendingRemoveName = ''

const filtered = computed(() => {
  if (!search.value) return volumes.value
  const q = search.value.toLowerCase()
  return volumes.value.filter((v) => v.name.toLowerCase().includes(q))
})

async function load() {
  loading.value = true
  unavailable.value = false
  try {
    volumes.value = await dockerVolumesApi.list()
  } catch {
    unavailable.value = true
  } finally {
    loading.value = false
  }
}

async function showDetail(vol: DockerVolumeSummary) {
  try {
    selectedDetail.value = await dockerVolumesApi.getDetail(vol.name)
    detailDialog.value = true
  } catch {
    // fallback — show summary fields as detail
    selectedDetail.value = { ...vol, options: {} }
    detailDialog.value = true
  }
}

function requestRemove(name: string) {
  pendingRemoveName = name
  confirmDialog.value = true
}

async function executeRemove() {
  if (!pendingRemoveName) return
  actionLoading.value = true
  try {
    await dockerVolumesApi.remove(pendingRemoveName)
    await load()
  } finally {
    actionLoading.value = false
    confirmDialog.value = false
    pendingRemoveName = ''
  }
}

onMounted(load)
</script>
