<template>
  <div>
    <v-row align="center" class="mb-4">
      <v-col>
        <v-breadcrumbs :items="['Docker', 'Imagens']" class="pa-0" />
        <h1 class="font-weight-bold text-h5 mt-1">Imagens</h1>
      </v-col>
      <v-col cols="auto">
        <div class="d-flex ga-2">
          <v-btn
            color="warning"
            :loading="pruneLoading"
            prepend-icon="mdi-broom"
            variant="tonal"
            @click="requestPrune"
          >
            Limpar não usadas
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
        placeholder="Buscar por tag ou repositório..."
        prepend-inner-icon="mdi-magnify"
        style="max-width: 400px"
        variant="outlined"
      />

      <v-progress-linear v-if="loading" indeterminate />

      <v-row v-else dense>
        <v-col
          v-for="img in filtered"
          :key="img.id"
          cols="12"
          md="4"
          sm="6"
        >
          <ImageCard
            :image="img"
            :loading="actionLoading"
            @detail="showDetail"
            @remove="requestRemove"
          />
        </v-col>
        <v-col v-if="filtered.length === 0" cols="12">
          <v-alert border="start" type="info" variant="tonal">Nenhuma imagem encontrada.</v-alert>
        </v-col>
      </v-row>
    </template>

    <ImageDetailDialog v-model="detailDialog" :detail="selectedDetail" />

    <DockerActionConfirmDialog
      v-model="confirmDialog"
      :loading="actionLoading || pruneLoading"
      :message="confirmMessage"
      @cancel="confirmDialog = false"
      @confirm="executeConfirmed"
    />
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import type { DockerImageDetail, DockerImageSummary } from '@/types/api'
import { dockerImagesApi } from '@/services/dockerService'
import ImageCard from '@/components/docker/ImageCard.vue'
import ImageDetailDialog from '@/components/docker/ImageDetailDialog.vue'
import DockerUnavailableBanner from '@/components/docker/DockerUnavailableBanner.vue'
import DockerActionConfirmDialog from '@/components/docker/DockerActionConfirmDialog.vue'

const images = ref<DockerImageSummary[]>([])
const loading = ref(false)
const actionLoading = ref(false)
const pruneLoading = ref(false)
const unavailable = ref(false)
const search = ref('')
const detailDialog = ref(false)
const confirmDialog = ref(false)
const confirmMessage = ref('')
const selectedDetail = ref<DockerImageDetail | null>(null)
let pendingConfirm: (() => Promise<void>) | null = null

const filtered = computed(() => {
  if (!search.value) return images.value
  const q = search.value.toLowerCase()
  return images.value.filter((img) =>
    img.repoTags.some((t) => t.toLowerCase().includes(q))
  )
})

async function load() {
  loading.value = true
  unavailable.value = false
  try {
    images.value = await dockerImagesApi.list()
  } catch {
    unavailable.value = true
  } finally {
    loading.value = false
  }
}

async function showDetail(img: DockerImageSummary) {
  try {
    selectedDetail.value = await dockerImagesApi.getDetail(img.id)
    detailDialog.value = true
  } catch {
    selectedDetail.value = null
  }
}

function requestRemove(id: string) {
  confirmMessage.value = 'Deseja remover esta imagem? Esta ação não pode ser desfeita.'
  pendingConfirm = async () => {
    await dockerImagesApi.remove(id)
    await load()
  }
  confirmDialog.value = true
}

function requestPrune() {
  confirmMessage.value = 'Deseja remover todas as imagens não utilizadas (dangling)?'
  pendingConfirm = async () => {
    pruneLoading.value = true
    try {
      await dockerImagesApi.prune()
      await load()
    } finally {
      pruneLoading.value = false
    }
  }
  confirmDialog.value = true
}

async function executeConfirmed() {
  if (!pendingConfirm) return
  actionLoading.value = true
  try {
    await pendingConfirm()
  } finally {
    actionLoading.value = false
    confirmDialog.value = false
    pendingConfirm = null
  }
}

onMounted(load)
</script>
