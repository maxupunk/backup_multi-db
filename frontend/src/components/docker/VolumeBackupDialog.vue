<template>
  <v-dialog v-model="model" max-width="480" :persistent="loading">
    <v-card rounded="lg">
      <v-card-title class="d-flex align-center ga-2 pt-4 px-4">
        <v-avatar color="success" rounded="lg" size="36">
          <v-icon icon="mdi-cloud-upload-outline" size="20" />
        </v-avatar>
        <div>
          <div class="text-subtitle-1 font-weight-bold">Backup do Volume</div>
          <div class="text-caption text-medium-emphasis text-truncate" style="max-width: 320px">{{ volumeName }}</div>
        </div>
      </v-card-title>

      <v-divider />

      <v-card-text class="pa-4">
        <!-- Mode selector -->
        <p class="text-body-2 text-medium-emphasis mb-3">Escolha como deseja salvar o backup:</p>

        <v-btn-toggle v-model="mode" class="mb-4 w-100" color="primary" density="comfortable" divided mandatory rounded="lg" variant="outlined">
          <v-btn class="flex-1-1" value="storage">
            <v-icon icon="mdi-cloud-outline" start />
            Armazenamento
          </v-btn>
          <v-btn class="flex-1-1" value="download">
            <v-icon icon="mdi-download-outline" start />
            Download
          </v-btn>
        </v-btn-toggle>

        <!-- Storage selection -->
        <template v-if="mode === 'storage'">
          <v-select
            v-model="selectedStorageId"
            clearable
            density="comfortable"
            :disabled="loadingStorages"
            hide-details="auto"
            item-title="name"
            item-value="id"
            :items="storages"
            label="Destino de armazenamento"
            :loading="loadingStorages"
            no-data-text="Nenhum armazenamento ativo encontrado"
            prepend-inner-icon="mdi-bucket-outline"
            variant="outlined"
          >
            <template #item="{ item, props: itemProps }">
              <v-list-item v-bind="itemProps">
                <template #prepend>
                  <v-icon :icon="storageIcon(item.raw.type)" size="20" />
                </template>
                <template #append>
                  <v-chip v-if="item.raw.isDefault" color="primary" label size="x-small" variant="tonal">Padrão</v-chip>
                </template>
              </v-list-item>
            </template>
          </v-select>

          <v-alert v-if="storages.length === 0 && !loadingStorages" border="start" class="mt-3" density="compact" type="warning" variant="tonal">
            Nenhum armazenamento ativo. Cadastre um em <strong>Armazenamentos</strong> primeiro.
          </v-alert>
        </template>

        <template v-else>
          <v-alert border="start" density="compact" icon="mdi-information-outline" type="info" variant="tonal">
            O arquivo <code>.tar.gz</code> será baixado diretamente para o seu computador.
          </v-alert>
        </template>
      </v-card-text>

      <v-divider />

      <v-card-actions class="pa-3 ga-2">
        <v-btn :disabled="loading" variant="text" @click="model = false">Cancelar</v-btn>
        <v-spacer />
        <v-btn
          color="success"
          :disabled="mode === 'storage' && !selectedStorageId"
          :loading="loading"
          prepend-icon="mdi-check"
          variant="flat"
          @click="confirm"
        >
          {{ mode === 'storage' ? 'Enviar Backup' : 'Baixar' }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import { ref, watch } from 'vue'
import type { StorageDestination } from '@/types/api'
import { storageDestinationsApi } from '@/services/api'

const model = defineModel<boolean>({ default: false })

const props = defineProps<{
  volumeName: string
  loading?: boolean
}>()

const emit = defineEmits<{
  (e: 'backup-storage', volumeName: string, storageId: number): void
  (e: 'backup-download', volumeName: string): void
}>()

const mode = ref<'storage' | 'download'>('storage')
const selectedStorageId = ref<number | null>(null)
const storages = ref<StorageDestination[]>([])
const loadingStorages = ref(false)

async function loadStorages() {
  loadingStorages.value = true
  try {
    const res = await storageDestinationsApi.list({ status: 'active', limit: 100 })
    storages.value = res.data?.data ?? []
    // Auto-select default if available
    const def = storages.value.find((s) => s.isDefault)
    if (def) selectedStorageId.value = def.id
  } catch {
    storages.value = []
  } finally {
    loadingStorages.value = false
  }
}

watch(model, (open) => {
  if (open) {
    mode.value = 'storage'
    selectedStorageId.value = null
    loadStorages()
  }
})

function storageIcon(type: string) {
  const icons: Record<string, string> = {
    local: 'mdi-harddisk',
    s3: 'mdi-aws',
    gcs: 'mdi-google-cloud',
    azure_blob: 'mdi-microsoft-azure',
    sftp: 'mdi-server-network',
  }
  return icons[type] ?? 'mdi-bucket-outline'
}

function confirm() {
  if (mode.value === 'storage' && selectedStorageId.value) {
    emit('backup-storage', props.volumeName, selectedStorageId.value)
  } else if (mode.value === 'download') {
    emit('backup-download', props.volumeName)
  }
}
</script>
