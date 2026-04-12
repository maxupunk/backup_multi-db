<template>
  <v-dialog v-model="model" max-width="440" persistent>
    <v-card>
      <v-card-title class="d-flex align-center pa-4">
        <v-icon class="mr-2" color="warning" icon="mdi-alert-outline" />
        Confirmar ação
      </v-card-title>
      <v-divider />

      <v-card-text class="pa-4">
        {{ message }}
      </v-card-text>

      <v-divider />
      <v-card-actions class="justify-end pa-3 ga-2">
        <v-btn :disabled="loading" variant="text" @click="emit('cancel')">Cancelar</v-btn>
        <v-btn
          :color="confirmColor"
          :loading="loading"
          variant="elevated"
          @click="emit('confirm')"
        >
          {{ confirmLabel }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
withDefaults(
  defineProps<{
    message: string
    confirmLabel?: string
    confirmColor?: string
    loading?: boolean
  }>(),
  {
    confirmLabel: 'Confirmar',
    confirmColor: 'error',
    loading: false,
  }
)

const model = defineModel<boolean>({ default: false })

const emit = defineEmits<{
  (e: 'confirm'): void
  (e: 'cancel'): void
}>()
</script>
