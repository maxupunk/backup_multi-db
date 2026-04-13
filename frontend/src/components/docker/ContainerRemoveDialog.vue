<template>
  <v-dialog v-model="model" max-width="440" persistent>
    <v-card>
      <v-card-title class="d-flex align-center pa-4 text-error">
        <v-icon class="mr-2" color="error" icon="mdi-delete-alert" />
        Remover Container
      </v-card-title>
      <v-divider />

      <v-card-text class="pa-4">
        <v-alert class="mb-4" color="error" density="compact" variant="tonal">
          <strong>Ação irreversível.</strong> O container será removido permanentemente. Dados não
          persistidos em volumes serão perdidos.
        </v-alert>

        <p class="text-body-2 mb-3">
          Para confirmar, digite o nome do container:
          <strong>{{ containerName }}</strong>
        </p>

        <v-text-field
          v-model="typedName"
          autofocus
          density="compact"
          hide-details="auto"
          label="Nome do container"
          placeholder="Digite o nome exato para confirmar"
          variant="outlined"
          @keyup.enter="canConfirm && emit('confirm', forceRemove)"
        />

        <v-checkbox
          v-model="forceRemove"
          class="mt-2"
          color="error"
          density="compact"
          hide-details
          label="Forçar remoção (mesmo que esteja em execução)"
        />
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
          @click="emit('confirm', forceRemove)"
        >
          Remover
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import { computed, ref, watch } from 'vue'

const model = defineModel<boolean>({ default: false })

const props = defineProps<{
  containerName: string
  loading?: boolean
}>()

const emit = defineEmits<{
  (e: 'cancel'): void
  (e: 'confirm', force: boolean): void
}>()

const typedName = ref('')
const forceRemove = ref(false)

const canConfirm = computed(() => typedName.value === props.containerName)

watch(model, (opened) => {
  if (opened) {
    typedName.value = ''
    forceRemove.value = false
  }
})
</script>
