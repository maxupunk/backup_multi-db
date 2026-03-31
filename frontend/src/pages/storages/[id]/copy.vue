<template>
  <div>
    <!-- Header -->
    <div class="d-flex align-center mb-6">
      <v-btn class="mr-3" icon="mdi-arrow-left" variant="text" :to="`/storages/${id}`" />
      <div>
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">
          Copiar — {{ storageName }}
        </h1>
        <p class="text-body-2 text-medium-emphasis">
          Copie arquivos para outro armazenamento
        </p>
      </div>
    </div>

    <!-- Active jobs -->
    <CopyJobProgress
      v-for="jobId in activeJobIds"
      :key="jobId"
      :job-id="jobId"
    />

    <!-- Copy form -->
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon class="mr-2" color="primary" icon="mdi-content-copy" />
        Nova Cópia
      </v-card-title>

      <v-card-text>
        <v-form ref="formRef">
          <v-row>
            <v-col cols="12" sm="6">
              <v-select
                v-model="form.destinationId"
                :items="destinationOptions"
                label="Destino *"
                prepend-inner-icon="mdi-bucket"
                :rules="[rules.required]"
              />
            </v-col>
          </v-row>

          <v-row>
            <v-col cols="12" sm="6">
              <v-combobox
                v-model="form.sourcePath"
                :items="sourceFolders"
                :loading="sourceFoldersLoading"
                clearable
                hint="Deixe vazio para copiar tudo"
                label="Path de Origem"
                no-data-text="Nenhuma pasta encontrada"
                persistent-hint
                prepend-inner-icon="mdi-folder-outline"
              />
            </v-col>
            <v-col cols="12" sm="6">
              <v-combobox
                v-model="form.destinationPath"
                :disabled="!form.destinationId"
                :items="destinationFolders"
                :loading="destinationFoldersLoading"
                clearable
                hint="Deixe vazio para raiz do destino"
                label="Path de Destino"
                no-data-text="Nenhuma pasta encontrada"
                persistent-hint
                prepend-inner-icon="mdi-folder"
              />
            </v-col>
          </v-row>

          <v-expansion-panels class="mt-4" variant="accordion">
            <v-expansion-panel>
              <v-expansion-panel-title>
                <v-icon class="mr-2" icon="mdi-cog" size="small" />
                Opções Avançadas
              </v-expansion-panel-title>
              <v-expansion-panel-text>
                <v-switch
                  v-model="form.dryRun"
                  color="info"
                  hide-details
                  label="Dry run (simular sem copiar)"
                />
                <v-switch
                  v-model="form.deleteExtraneous"
                  color="warning"
                  hide-details
                  label="Deletar extras no destino (sync mode)"
                />
                <v-alert v-if="form.deleteExtraneous" class="mt-2" density="compact" type="warning" variant="tonal">
                  Arquivos no destino que não existem na origem serão <strong>removidos</strong>.
                </v-alert>
              </v-expansion-panel-text>
            </v-expansion-panel>
          </v-expansion-panels>
        </v-form>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn
          color="primary"
          :loading="starting"
          prepend-icon="mdi-play"
          variant="flat"
          @click="startCopy"
        >
          Iniciar Cópia
        </v-btn>
      </v-card-actions>
    </v-card>
  </div>
</template>

<script lang="ts" setup>
import type { Storage } from '@/types/api'
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useDisplay } from 'vuetify'
import { ApiError, storagesApi } from '@/services/api'
import { useStoragesStore } from '@/stores/storages'
import { useNotifier } from '@/composables/useNotifier'
import { useStorageFolders } from '@/composables/useStorageFolders'
import CopyJobProgress from '@/components/storages/CopyJobProgress.vue'

const route = useRoute()
const { mdAndUp } = useDisplay()
const storagesStore = useStoragesStore()
const notify = useNotifier()

const id = Number((route.params as { id: string }).id)
const storageName = ref('')
const formRef = ref()
const starting = ref(false)
const activeJobIds = ref<string[]>([])

const {
  folders: sourceFolders,
  loading: sourceFoldersLoading,
  loadFolders: loadSourceFolders,
} = useStorageFolders()

const {
  folders: destinationFolders,
  loading: destinationFoldersLoading,
  loadFolders: loadDestinationFolders,
  reset: resetDestinationFolders,
} = useStorageFolders()

const form = ref({
  destinationId: null as number | null,
  sourcePath: '',
  destinationPath: '',
  dryRun: false,
  deleteExtraneous: false,
})

const destinationOptions = computed(() =>
  storagesStore.activeStorages
    .filter((s) => s.id !== id)
    .map((s) => ({ title: s.name, value: s.id })),
)

watch(() => form.value.destinationId, (destinationId) => {
  form.value.destinationPath = ''
  if (destinationId) {
    loadDestinationFolders(destinationId)
  } else {
    resetDestinationFolders()
  }
})

const rules = {
  required: (v: unknown) => !!v || 'Campo obrigatório',
}

async function startCopy () {
  const validation = await formRef.value?.validate?.()
  if (validation && 'valid' in validation && validation.valid === false) return
  if (!form.value.destinationId) return

  starting.value = true
  try {
    const response = await storagesApi.startCopy(id, {
      destinationId: form.value.destinationId,
      sourcePath: form.value.sourcePath || undefined,
      destinationPath: form.value.destinationPath || undefined,
      dryRun: form.value.dryRun || undefined,
      deleteExtraneous: form.value.deleteExtraneous || undefined,
    })

    const jobId = response.data?.jobId
    if (jobId) {
      activeJobIds.value.push(jobId)
      notify('Cópia iniciada com sucesso', 'info')
    }
  } catch (error) {
    const msg = error instanceof ApiError ? error.message : 'Erro ao iniciar cópia'
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

  loadSourceFolders(id)

  try {
    await storagesStore.fetchAll({ limit: 100 })
  } catch { /* ignore */ }
})
</script>
