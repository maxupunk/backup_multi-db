<template>
  <v-dialog
    v-model="model"
    :max-width="target?.isDirectory ? 520 : 440"
    :persistent="Boolean(target?.isDirectory)"
  >
    <v-card>
      <v-card-title class="d-flex align-center pa-4" :class="target?.isDirectory ? 'text-error' : ''">
        <v-icon
          class="mr-2"
          :color="target?.isDirectory ? 'error' : 'warning'"
          :icon="target?.isDirectory ? 'mdi-folder-remove' : 'mdi-delete-alert'"
        />
        {{ target?.isDirectory ? 'Excluir Pasta' : 'Excluir Arquivo' }}
      </v-card-title>

      <v-divider />

      <v-card-text class="pa-4">
        <v-alert
          class="mb-4"
          :color="target?.isDirectory ? 'error' : 'warning'"
          density="compact"
          variant="tonal"
        >
          <template v-if="target?.isDirectory">
            <strong>Ação irreversível.</strong> A pasta e todo o conteúdo abaixo dela serão removidos permanentemente.
          </template>
          <template v-else>
            <strong>Ação irreversível.</strong> O arquivo será removido permanentemente.
          </template>
        </v-alert>

        <template v-if="target?.isDirectory">
          <p class="text-body-2 mb-3">
            Para confirmar, digite o caminho exato da pasta:
            <code class="font-weight-bold">{{ confirmationTarget }}</code>
          </p>

          <v-text-field
            v-model="typedConfirmation"
            autofocus
            density="compact"
            :error="typedConfirmation.length > 0 && !canConfirm"
            hide-details="auto"
            :hint="canConfirm ? 'Confirmação válida' : ''"
            label="Confirmação"
            persistent-hint
            placeholder="Digite o caminho exato para confirmar"
            variant="outlined"
            @keyup.enter="canConfirm && emit('confirm')"
          />
        </template>

        <p v-else class="text-body-2 mb-0">
          Tem certeza que deseja excluir o arquivo
          <strong>{{ target?.name }}</strong>?
        </p>
      </v-card-text>

      <v-divider />

      <v-card-actions class="pa-3">
        <v-spacer />
        <v-btn variant="text" @click="emit('cancel')">Cancelar</v-btn>
        <v-btn
          color="error"
          :disabled="!canConfirm || loading"
          :loading="loading"
          prepend-icon="mdi-delete"
          variant="flat"
          @click="emit('confirm')"
        >
          Excluir
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import type { BucketObject } from '@/types/api'
import { computed, ref, watch } from 'vue'

const model = defineModel<boolean>({ default: false })

const props = defineProps<{
  target: BucketObject | null
  loading?: boolean
}>()

const emit = defineEmits<{
  (e: 'cancel'): void
  (e: 'confirm'): void
}>()

const typedConfirmation = ref('')

const confirmationTarget = computed(() => props.target?.key.replace(/\/$/, '') ?? '')

const canConfirm = computed(() => {
  if (!props.target) {
    return false
  }

  if (!props.target.isDirectory) {
    return true
  }

  return typedConfirmation.value === confirmationTarget.value
})

watch(model, (opened) => {
  if (opened) {
    typedConfirmation.value = ''
  }
})
</script>