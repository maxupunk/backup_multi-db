<template>
  <v-dialog v-model="model" max-width="540">
    <v-card>
      <v-card-title class="d-flex align-center ga-2 pt-4 px-4">
        <v-avatar :color="all ? 'error' : 'warning'" size="36">
          <v-icon :icon="all ? 'mdi-delete-sweep' : 'mdi-broom'" />
        </v-avatar>
        <span class="text-h6 font-weight-bold">
          {{ all ? 'Limpeza Completa (docker system prune -a)' : 'Limpar Cache do Docker' }}
        </span>
      </v-card-title>

      <v-card-text class="px-4 py-3">
        <p class="text-body-2 mb-3">
          {{
            all
              ? 'Esta ação executará uma limpeza profunda equivalente a "docker system prune -a". Serão removidos todos os containers parados, redes não utilizadas, cache de build (BuildKit) e todas as imagens não associadas a um container em execução.'
              : 'Esta ação limpará o cache do Docker, removendo containers parados, redes não utilizadas, cache de build temporário e imagens dangling (camadas sem tag).'
          }}
        </p>

        <v-checkbox
          v-model="includeVolumes"
          color="error"
          density="comfortable"
          hide-details
          label="Remover também volumes não utilizados (órfãos)"
        />
        <p v-if="includeVolumes" class="text-caption text-error mt-1 ml-8">
          Atenção: volumes órfãos não associados a nenhum container serão excluídos permanentemente.
        </p>
      </v-card-text>

      <v-divider />

      <v-card-actions class="pa-3">
        <v-spacer />
        <v-btn :disabled="loading" variant="text" @click="model = false">
          Cancelar
        </v-btn>
        <v-btn
          :color="all ? 'error' : 'warning'"
          :loading="loading"
          variant="flat"
          @click="executePrune"
        >
          Confirmar Limpeza
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import { ref, watch } from 'vue'
import { dockerSystemApi } from '@/services/dockerService'
import { formatBytes } from '@/utils/format'
import { useNotifier } from '@/composables/useNotifier'

const props = withDefaults(
  defineProps<{
    all?: boolean
  }>(),
  {
    all: false,
  }
)

const emit = defineEmits<{
  (e: 'success'): void
}>()

const model = defineModel<boolean>({ default: false })
const notify = useNotifier()

const includeVolumes = ref(false)
const loading = ref(false)

watch(model, (opened) => {
  if (opened) {
    includeVolumes.value = false
  }
})

async function executePrune(): Promise<void> {
  loading.value = true
  try {
    const res = await dockerSystemApi.prune({
      all: props.all,
      volumes: includeVolumes.value,
    })
    const freed = formatBytes(res.spaceReclaimed)
    const counts: string[] = []
    if (res.containersDeleted && res.containersDeleted.length > 0) {
      counts.push(`${res.containersDeleted.length} container(s)`)
    }
    if (res.imagesDeleted && res.imagesDeleted.length > 0) {
      counts.push(`${res.imagesDeleted.length} imagem(ns)`)
    }
    if (res.networksDeleted && res.networksDeleted.length > 0) {
      counts.push(`${res.networksDeleted.length} rede(s)`)
    }
    if (res.volumesDeleted && res.volumesDeleted.length > 0) {
      counts.push(`${res.volumesDeleted.length} volume(s)`)
    }
    if (res.buildCacheDeleted && res.buildCacheDeleted.length > 0) {
      counts.push(`${res.buildCacheDeleted.length} cache(s) de build`)
    }

    const detailMsg = counts.length > 0 ? ` (${counts.join(', ')})` : ''
    notify(`Limpeza concluída com sucesso! ${freed} liberados${detailMsg}.`, 'success')
    model.value = false
    emit('success')
  } catch (error) {
    notify(error instanceof Error ? error.message : 'Erro ao executar limpeza.', 'error')
  } finally {
    loading.value = false
  }
}
</script>
