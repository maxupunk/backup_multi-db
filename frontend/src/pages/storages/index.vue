<template>
  <div>
    <!-- Header -->
    <v-row align="center" class="mb-6">
      <v-col class="flex-grow-1" cols="12" sm="auto">
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">Armazenamentos</h1>
        <p class="text-body-2 text-medium-emphasis">
          Gerencie seus destinos de armazenamento e explore buckets
        </p>
      </v-col>

      <v-col cols="12" sm="auto">
        <v-btn :block="!smAndUp" color="primary" prepend-icon="mdi-plus" to="/storages/new">
          Novo Armazenamento
        </v-btn>
      </v-col>
    </v-row>

    <!-- Filters -->
    <v-card class="mb-6">
      <v-card-text>
        <v-row>
          <v-col cols="12" sm="4">
            <v-text-field
              v-model="filters.search"
              clearable
              density="comfortable"
              hide-details
              label="Buscar"
              prepend-inner-icon="mdi-magnify"
              variant="outlined"
              @update:model-value="debouncedLoad"
            />
          </v-col>

          <v-col cols="12" sm="4">
            <v-select
              v-model="filters.provider"
              clearable
              density="comfortable"
              hide-details
              :items="providerFilterOptions"
              label="Provider"
              variant="outlined"
              @update:model-value="loadStorages"
            />
          </v-col>

          <v-col cols="12" sm="4">
            <v-select
              v-model="filters.status"
              clearable
              density="comfortable"
              hide-details
              :items="statusOptions"
              label="Status"
              variant="outlined"
              @update:model-value="loadStorages"
            />
          </v-col>
        </v-row>
      </v-card-text>
    </v-card>

    <!-- List -->
    <v-card>
      <v-data-table
        class="elevation-0"
        :headers="tableHeaders"
        :items="storagesStore.storages"
        :items-per-page="mdAndUp ? 15 : 5"
        :loading="storagesStore.loading"
        :mobile="!mdAndUp"
      >
        <template #item.provider="{ item }">
          <div class="d-flex align-center">
            <StorageProviderIcon class="mr-2" :provider="item.provider" size="small" />
            <span>{{ getProviderLabel(item.provider) }}</span>
          </div>
        </template>

        <template #item.status="{ item }">
          <v-chip :color="item.status === 'active' ? 'success' : 'grey'" label size="small">
            {{ item.status === 'active' ? 'Ativo' : 'Inativo' }}
          </v-chip>
        </template>

        <template #item.isDefault="{ item }">
          <v-chip v-if="item.isDefault" color="primary" label size="small" variant="tonal">
            Padrão
          </v-chip>
        </template>

        <template #item.createdAt="{ item }">
          <span class="text-medium-emphasis">
            {{ formatDateTimePtBR(item.createdAt) }}
          </span>
        </template>

        <template #item.actions="{ item }">
          <template v-if="mdAndUp">
            <v-btn icon="mdi-eye" size="small" variant="text" :to="`/storages/${item.id}`">
              <v-icon icon="mdi-eye" />
              <v-tooltip activator="parent" location="bottom">Detalhes</v-tooltip>
            </v-btn>
            <v-btn icon="mdi-folder-search" size="small" variant="text" :to="`/storages/${item.id}/explore`">
              <v-icon icon="mdi-folder-search" />
              <v-tooltip activator="parent" location="bottom">Explorar</v-tooltip>
            </v-btn>

            <v-menu location="bottom end">
              <template #activator="{ props }">
                <v-btn v-bind="props" icon="mdi-dots-vertical" size="small" variant="text" />
              </template>
              <v-list density="compact">
                <v-list-item
                  prepend-icon="mdi-content-copy"
                  title="Copiar para..."
                  :to="`/storages/${item.id}/copy`"
                />
                <v-list-item
                  prepend-icon="mdi-archive-arrow-down"
                  title="Download"
                  :to="`/storages/${item.id}/download`"
                />
                <v-divider class="my-1" />
                <v-list-item
                  base-color="error"
                  prepend-icon="mdi-delete"
                  title="Excluir"
                  @click="confirmDelete(item)"
                />
              </v-list>
            </v-menu>
          </template>

          <v-menu v-else location="bottom end">
            <template #activator="{ props }">
              <v-btn v-bind="props" icon="mdi-dots-vertical" variant="text" />
            </template>
            <v-list density="compact">
              <v-list-item prepend-icon="mdi-eye" title="Detalhes" :to="`/storages/${item.id}`" />
              <v-list-item prepend-icon="mdi-folder-search" title="Explorar" :to="`/storages/${item.id}/explore`" />
              <v-list-item prepend-icon="mdi-content-copy" title="Copiar para..." :to="`/storages/${item.id}/copy`" />
              <v-list-item prepend-icon="mdi-archive-arrow-down" title="Download" :to="`/storages/${item.id}/download`" />
              <v-divider class="my-1" />
              <v-list-item base-color="error" prepend-icon="mdi-delete" title="Excluir" @click="confirmDelete(item)" />
            </v-list>
          </v-menu>
        </template>

        <template #no-data>
          <div class="text-center py-8 text-medium-emphasis">
            <v-icon class="mb-2" icon="mdi-bucket" size="48" />
            <p>Nenhum armazenamento encontrado</p>
            <v-btn class="mt-4" color="primary" prepend-icon="mdi-plus" to="/storages/new" variant="tonal">
              Criar primeiro armazenamento
            </v-btn>
          </div>
        </template>
      </v-data-table>
    </v-card>

    <!-- Delete dialog -->
    <v-dialog v-model="deleteDialog" max-width="420">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon class="mr-2" color="error" icon="mdi-alert" />
          Confirmar Exclusão
        </v-card-title>

        <v-card-text>
          Tem certeza que deseja excluir o armazenamento
          <strong>{{ storageToDelete?.name }}</strong>?
        </v-card-text>

        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="deleteDialog = false">Cancelar</v-btn>
          <v-btn color="error" :loading="deleting" variant="flat" @click="deleteStorage">
            Excluir
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script lang="ts" setup>
import type { Storage, StorageProvider } from '@/types/api'
import { onMounted, reactive, ref } from 'vue'
import { useDisplay } from 'vuetify'
import { ApiError } from '@/services/api'
import { useStoragesStore } from '@/stores/storages'
import { useDebouncedFn } from '@/composables/useDebouncedFn'
import { useNotifier } from '@/composables/useNotifier'
import { getProviderLabel, storageProviderOptions } from '@/ui/storage'
import { formatDateTimePtBR } from '@/utils/format'
import StorageProviderIcon from '@/components/storages/StorageProviderIcon.vue'

const { mdAndUp, smAndUp } = useDisplay()
const storagesStore = useStoragesStore()
const notify = useNotifier()

const filters = reactive<{
  search: string
  provider: StorageProvider | null
  status: string | null
}>({
  search: '',
  provider: null,
  status: null,
})

const deleteDialog = ref(false)
const deleting = ref(false)
const storageToDelete = ref<Storage | null>(null)

const tableHeaders = [
  { title: 'Nome', key: 'name', sortable: true },
  { title: 'Provider', key: 'provider', sortable: true },
  { title: 'Status', key: 'status', sortable: true },
  { title: '', key: 'isDefault', sortable: false },
  { title: 'Criado em', key: 'createdAt', sortable: true },
  { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
]

const providerFilterOptions = storageProviderOptions.map((o) => ({
  title: o.title,
  value: o.value,
}))

const statusOptions = [
  { title: 'Ativo', value: 'active' },
  { title: 'Inativo', value: 'inactive' },
]

async function loadStorages () {
  try {
    await storagesStore.fetchAll({
      search: filters.search || undefined,
      provider: filters.provider ?? undefined,
      status: filters.status ?? undefined,
    })
  } catch {
    notify('Erro ao carregar armazenamentos', 'error')
  }
}

const debouncedLoad = useDebouncedFn(loadStorages, 300)

function confirmDelete (item: Storage) {
  storageToDelete.value = item
  deleteDialog.value = true
}

async function deleteStorage () {
  if (!storageToDelete.value) return
  deleting.value = true
  try {
    await storagesStore.remove(storageToDelete.value.id)
    notify('Armazenamento removido com sucesso', 'success')
    deleteDialog.value = false
    storageToDelete.value = null
    loadStorages()
  } catch (error) {
    const msg = error instanceof ApiError ? error.message : 'Erro ao remover armazenamento'
    notify(msg, 'error')
  } finally {
    deleting.value = false
  }
}

onMounted(loadStorages)
</script>
