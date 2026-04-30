<template>
  <div>
    <!-- Breadcrumb -->
    <v-breadcrumbs class="px-0 pt-0">
      <v-breadcrumbs-item
        v-for="(segment, i) in explorerStore.breadcrumbs"
        :key="segment.path"
        :disabled="i === explorerStore.breadcrumbs.length - 1"
        @click="i < explorerStore.breadcrumbs.length - 1 && explorerStore.navigateTo(segment.path)"
      >
        <v-icon v-if="i === 0" icon="mdi-bucket" size="small" class="mr-1" />
        {{ segment.label }}
      </v-breadcrumbs-item>
    </v-breadcrumbs>

    <!-- Toolbar -->
    <div class="d-flex align-center mb-4 ga-2">
      <v-btn
        :disabled="!explorerStore.currentPath"
        icon="mdi-arrow-up"
        size="small"
        variant="tonal"
        @click="explorerStore.navigateUp()"
      >
        <v-icon icon="mdi-arrow-up" />
        <v-tooltip activator="parent" location="bottom">Voltar</v-tooltip>
      </v-btn>

      <v-btn
        icon="mdi-refresh"
        :loading="explorerStore.loading"
        size="small"
        variant="tonal"
        @click="explorerStore.refresh()"
      >
        <v-icon icon="mdi-refresh" />
        <v-tooltip activator="parent" location="bottom">Atualizar</v-tooltip>
      </v-btn>

      <v-text-field
        v-model="searchFilter"
        clearable
        density="compact"
        hide-details
        placeholder="Filtrar por nome..."
        prepend-inner-icon="mdi-magnify"
        style="max-width: 300px"
        variant="outlined"
      />

      <v-spacer />

      <span class="text-caption text-medium-emphasis">
        {{ filteredObjects.length }} itens
      </span>
    </div>

    <!-- Table -->
    <v-data-table
      class="elevation-0"
      density="comfortable"
      :headers="headers"
      :items="filteredObjects"
      :items-per-page="50"
      :loading="explorerStore.loading"
      :mobile="!mdAndUp"
      @click:row="handleRowClick"
    >
      <template #item.name="{ item }">
        <div class="d-flex align-center cursor-pointer">
          <v-icon
            class="mr-2"
            :color="item.isDirectory ? 'warning' : 'grey'"
            :icon="getFileIcon(item.name, item.isDirectory)"
            size="small"
          />
          <span :class="item.isDirectory ? 'font-weight-medium' : ''">
            {{ item.name }}
          </span>
        </div>
      </template>

      <template #item.size="{ item }">
        <span class="text-medium-emphasis">
          {{ item.isDirectory ? '-' : formatFileSize(item.size) }}
        </span>
      </template>

      <template #item.lastModified="{ item }">
        <span class="text-medium-emphasis">
          {{ item.lastModified ? formatDateTimePtBR(item.lastModified) : '-' }}
        </span>
      </template>

      <template #item.actions="{ item }">
        <v-menu location="bottom end">
          <template #activator="{ props: menuProps }">
            <v-btn
              icon="mdi-dots-vertical"
              size="small"
              variant="text"
              v-bind="menuProps"
              @click.stop
            />
          </template>
          <v-list density="compact">
            <v-list-item
              v-if="!item.isDirectory"
              prepend-icon="mdi-information"
              title="Detalhes"
              @click.stop="emit('show-details', item)"
            />
            <v-list-item
              base-color="error"
              prepend-icon="mdi-delete"
              :title="item.isDirectory ? 'Excluir pasta' : 'Excluir arquivo'"
              @click.stop="openDeleteDialog(item)"
            />
          </v-list>
        </v-menu>
      </template>

      <template #no-data>
        <div class="text-center py-8 text-medium-emphasis">
          <v-icon class="mb-2" icon="mdi-folder-open" size="48" />
          <p>{{ explorerStore.loading ? 'Carregando...' : 'Pasta vazia' }}</p>
        </div>
      </template>

      <template #bottom>
        <div v-if="explorerStore.hasMore" class="text-center py-4">
          <v-btn
            color="primary"
            :loading="explorerStore.loading"
            variant="tonal"
            @click="explorerStore.loadMore()"
          >
            Carregar mais
          </v-btn>
        </div>
      </template>
    </v-data-table>

    <StorageExplorerDeleteDialog
      v-model="deleteDialog"
      :loading="deleteLoading"
      :target="objectToDelete"
      @cancel="deleteDialog = false"
      @confirm="deleteObject"
    />
  </div>
</template>

<script lang="ts" setup>
import type { BucketObject } from '@/types/api'
import { computed, ref, watch } from 'vue'
import { useDisplay } from 'vuetify'
import { ApiError } from '@/services/api'
import { useNotifier } from '@/composables/useNotifier'
import { useStorageExplorerStore } from '@/stores/storage-explorer'
import { getFileIcon } from '@/ui/storage'
import { formatDateTimePtBR, formatFileSize } from '@/utils/format'
import StorageExplorerDeleteDialog from '@/components/storages/StorageExplorerDeleteDialog.vue'

const { mdAndUp } = useDisplay()
const explorerStore = useStorageExplorerStore()
const notify = useNotifier()

const emit = defineEmits<{
  'show-details': [object: BucketObject]
}>()

const searchFilter = ref('')
const deleteDialog = ref(false)
const deleteLoading = ref(false)
const objectToDelete = ref<BucketObject | null>(null)

const headers = [
  { title: 'Nome', key: 'name', sortable: true },
  { title: 'Tamanho', key: 'size', sortable: true, width: 120 },
  { title: 'Última modificação', key: 'lastModified', sortable: true, width: 180 },
  { title: '', key: 'actions', sortable: false, width: 60 },
]

const filteredObjects = computed(() => {
  if (!searchFilter.value) return explorerStore.objects
  const term = searchFilter.value.toLowerCase()
  return explorerStore.objects.filter((obj) =>
    obj.name.toLowerCase().includes(term),
  )
})

function handleRowClick (_event: Event, row: { item: BucketObject }) {
  if (row.item.isDirectory) {
    explorerStore.navigateTo(row.item.key)
  }
}

function openDeleteDialog (item: BucketObject) {
  objectToDelete.value = item
  deleteDialog.value = true
}

async function deleteObject () {
  if (!objectToDelete.value) return

  const target = objectToDelete.value
  deleteLoading.value = true

  try {
    await explorerStore.removeObject(target.key, target.isDirectory)
    notify(target.isDirectory ? 'Pasta excluída com sucesso' : 'Arquivo excluído com sucesso', 'success')
    deleteDialog.value = false
  } catch (error) {
    const fallbackMessage = target.isDirectory ? 'Erro ao excluir pasta' : 'Erro ao excluir arquivo'
    const message = error instanceof ApiError ? error.message : fallbackMessage
    notify(message, 'error')
  } finally {
    deleteLoading.value = false
  }
}

watch(deleteDialog, (opened) => {
  if (!opened) {
    objectToDelete.value = null
  }
})
</script>

<style scoped>
.cursor-pointer {
  cursor: pointer;
}
</style>
