<template>
  <div>
    <!-- Header -->
    <div class="d-flex align-center mb-6">
      <v-btn class="mr-3" icon="mdi-arrow-left" variant="text" :to="`/storages/${id}`" />
      <div>
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">
          Download — {{ storageName }}
        </h1>
        <p class="text-body-2 text-medium-emphasis">
          Gere um arquivo compactado do conteúdo do armazenamento
        </p>
      </div>
    </div>

    <!-- Active archives -->
    <ArchiveProgress
      v-for="jobId in activeArchiveIds"
      :key="jobId"
      :job-id="jobId"
    />

    <!-- Archive form -->
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon class="mr-2" color="secondary" icon="mdi-archive-arrow-down" />
        Gerar Archive
      </v-card-title>

      <v-card-text>
        <!-- Path selector -->
        <v-combobox
          v-model="archivePath"
          clearable
          hint="Deixe vazio para incluir todo o conteúdo do bucket"
          :items="availablePaths"
          item-title="label"
          item-value="value"
          label="Path (opcional)"
          :loading="loadingPaths"
          no-data-text="Nenhum diretório encontrado"
          persistent-hint
          prepend-inner-icon="mdi-folder-outline"
          return-object
        >
          <template #item="{ item, props: itemProps }">
            <v-list-item v-bind="itemProps">
              <template #prepend>
                <v-icon color="warning" icon="mdi-folder" size="small" />
              </template>
            </v-list-item>
          </template>

          <template #selection="{ item }">
            <span>{{ typeof item === 'string' ? item : item?.value }}</span>
          </template>
        </v-combobox>

        <!-- Path chips (quick selection) -->
        <div v-if="availablePaths.length > 0" class="mt-3">
          <p class="text-caption text-medium-emphasis mb-2">
            Diretórios disponíveis:
          </p>
          <v-chip-group v-model="selectedChip" column>
            <v-chip
              :color="selectedChip === ALL_CONTENT_VALUE ? 'secondary' : undefined"
              filter
              size="small"
              :value="ALL_CONTENT_VALUE"
              @click="selectPath(null)"
            >
              <v-icon icon="mdi-bucket" start />
              Todo o conteúdo
            </v-chip>
            <v-chip
              v-for="p in availablePaths"
              :key="p.value"
              filter
              size="small"
              :value="p.value"
              @click="selectPath(p.value)"
            >
              <v-icon icon="mdi-folder" start />
              {{ p.label }}
            </v-chip>
          </v-chip-group>
        </div>

        <v-alert class="mt-4" density="compact" type="info" variant="tonal">
          O archive será gerado como <strong>.tar.gz</strong> e ficará disponível para download por <strong>15 minutos</strong>.
        </v-alert>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn
          color="secondary"
          :loading="starting"
          prepend-icon="mdi-archive-arrow-down"
          variant="flat"
          @click="startArchive"
        >
          Gerar Archive
        </v-btn>
      </v-card-actions>
    </v-card>
  </div>
</template>

<script lang="ts" setup>
import type { Storage } from '@/types/api'
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useDisplay } from 'vuetify'
import { ApiError, storagesApi } from '@/services/api'
import { useNotifier } from '@/composables/useNotifier'
import ArchiveProgress from '@/components/storages/ArchiveProgress.vue'

interface PathOption {
  label: string
  value: string
}

const ALL_CONTENT_VALUE = '__all__'

const route = useRoute()
const { mdAndUp } = useDisplay()
const notify = useNotifier()

const id = Number((route.params as { id: string }).id)
const storageName = ref('')
const archivePath = ref<PathOption | string | null>(null)
const starting = ref(false)
const activeArchiveIds = ref<string[]>([])
const availablePaths = ref<PathOption[]>([])
const loadingPaths = ref(false)

const selectedChip = computed(() => {
  if (!archivePath.value) return ALL_CONTENT_VALUE
  const val = typeof archivePath.value === 'string' ? archivePath.value : archivePath.value.value
  return val || ALL_CONTENT_VALUE
})

function resolvedPath (): string | undefined {
  if (!archivePath.value) return undefined
  const raw = typeof archivePath.value === 'string' ? archivePath.value : archivePath.value.value
  return raw.trim() || undefined
}

function selectPath (path: string | null) {
  if (!path) {
    archivePath.value = null
    return
  }
  const found = availablePaths.value.find(p => p.value === path)
  archivePath.value = found ?? path
}

async function loadAvailablePaths () {
  loadingPaths.value = true
  try {
    const response = await storagesApi.browse(id)
    const objects = response.data?.objects ?? []
    availablePaths.value = objects
      .filter(obj => obj.isDirectory)
      .map(obj => ({ label: obj.name, value: obj.key }))
  } catch {
    /* silently ignore — field remains a free-text input */
  } finally {
    loadingPaths.value = false
  }
}

async function startArchive () {
  starting.value = true
  try {
    const response:any = await storagesApi.startArchive(id, resolvedPath())
    const jobId = response.data?.id
    if (jobId) {
      activeArchiveIds.value.push(jobId)
      notify('Geração de archive iniciada', 'info')
    }
  } catch (error) {
    const msg = error instanceof ApiError ? error.message : 'Erro ao iniciar archive'
    notify(msg, 'error')
  } finally {
    starting.value = false
  }
}

onMounted(async () => {
  try {
    const response = await storagesApi.get(id)
    const data = response.data as Storage | undefined
    storageName.value = data?.name ?? `#${id}`
  } catch { /* ignore */ }

  await loadAvailablePaths()
})
</script>
