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
        <v-text-field
          v-model="archivePath"
          hint="Deixe vazio para incluir todo o bucket"
          label="Path (opcional)"
          persistent-hint
          prepend-inner-icon="mdi-folder-outline"
        />

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
import { onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useDisplay } from 'vuetify'
import { ApiError, storagesApi } from '@/services/api'
import { useNotifier } from '@/composables/useNotifier'
import ArchiveProgress from '@/components/storages/ArchiveProgress.vue'

const route = useRoute()
const { mdAndUp } = useDisplay()
const notify = useNotifier()

const id = Number((route.params as { id: string }).id)
const storageName = ref('')
const archivePath = ref('')
const starting = ref(false)
const activeArchiveIds = ref<string[]>([])

async function startArchive () {
  starting.value = true
  try {
    const response = await storagesApi.startArchive(id, archivePath.value || undefined)
    const jobId = response.data?.jobId
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
})
</script>
