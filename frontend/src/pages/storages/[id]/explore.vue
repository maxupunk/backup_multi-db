<template>
  <div>
    <!-- Header -->
    <div class="d-flex align-center mb-6">
      <v-btn class="mr-3" icon="mdi-arrow-left" variant="text" :to="`/storages/${id}`" />
      <div class="flex-grow-1">
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">
          Explorar — {{ storageName }}
        </h1>
        <p class="text-body-2 text-medium-emphasis">
          Navegue pelos arquivos do armazenamento
        </p>
      </div>
    </div>

    <v-card>
      <v-card-text>
        <BucketExplorer @show-details="showObjectDetails" />
      </v-card-text>
    </v-card>

    <!-- Details dialog -->
    <v-dialog v-model="detailsDialog" max-width="500">
      <v-card v-if="selectedObject">
        <v-card-title class="d-flex align-center">
          <v-icon class="mr-2" color="primary" icon="mdi-information" />
          Detalhes do Arquivo
        </v-card-title>
        <v-card-text>
          <v-list density="compact">
            <v-list-item :subtitle="selectedObject.name" title="Nome" />
            <v-list-item :subtitle="selectedObject.key" title="Chave" />
            <v-list-item :subtitle="selectedObject.size !== null ? formatFileSize(selectedObject.size) : '-'" title="Tamanho" />
            <v-list-item :subtitle="selectedObject.lastModified ? formatDateTimePtBR(selectedObject.lastModified) : '-'" title="Última modificação" />
            <v-list-item v-if="selectedObject.etag" :subtitle="selectedObject.etag" title="ETag" />
          </v-list>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="detailsDialog = false">Fechar</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script lang="ts" setup>
import type { BucketObject, Storage } from '@/types/api'
import { onMounted, onUnmounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useDisplay } from 'vuetify'
import { storagesApi } from '@/services/api'
import { useStorageExplorerStore } from '@/stores/storage-explorer'
import { formatDateTimePtBR, formatFileSize } from '@/utils/format'
import BucketExplorer from '@/components/storages/BucketExplorer.vue'

const route = useRoute()
const { mdAndUp } = useDisplay()
const explorerStore = useStorageExplorerStore()

const id = Number((route.params as { id: string }).id)
const storageName = ref('')
const detailsDialog = ref(false)
const selectedObject = ref<BucketObject | null>(null)

function showObjectDetails (obj: BucketObject) {
  selectedObject.value = obj
  detailsDialog.value = true
}

onMounted(async () => {
  try {
    const response = await storagesApi.get(id)
    const data = response.data as Storage | undefined
    storageName.value = data?.name ?? `#${id}`
  } catch { /* ignore */ }

  explorerStore.browse(id)
})

onUnmounted(() => {
  explorerStore.reset()
})
</script>
