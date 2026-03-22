<template>
  <v-dialog v-model="isOpen" max-width="420" persistent @keydown.enter="submit">
    <v-card>
      <v-card-title class="d-flex align-center gap-2 pa-4">
        <v-icon color="primary" icon="mdi-database-plus-outline" />
        Criar Banco de Dados
      </v-card-title>

      <v-divider />

      <v-card-text class="pa-4">
        <p class="text-body-2 text-medium-emphasis mb-4">
          O banco de dados será criado na conexão
          <strong>{{ connectionName }}</strong>.
        </p>

        <v-text-field
          ref="inputRef"
          v-model="databaseName"
          autofocus
          clearable
          density="comfortable"
          :error-messages="errorMessage ? [errorMessage] : []"
          hint="Apenas letras, números, underscores e hífens. Deve começar com letra ou underscore."
          label="Nome do banco de dados"
          persistent-hint
          :placeholder="placeholder"
          variant="outlined"
          @input="errorMessage = ''"
        />
      </v-card-text>

      <v-divider />

      <v-card-actions class="pa-3">
        <v-btn :disabled="loading" variant="text" @click="close">
          Cancelar
        </v-btn>
        <v-spacer />
        <v-btn
          color="primary"
          :disabled="!isValid"
          :loading="loading"
          variant="flat"
          @click="submit"
        >
          <v-icon start icon="mdi-plus" />
          Criar
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import { computed, ref, watch } from 'vue'
import { connectionsApi } from '@/services/api'
import { useNotifier } from '@/composables/useNotifier'

// ── Props / Emits ─────────────────────────────────────────────────────────────

const props = defineProps<{
  connectionId: number | null
  connectionName: string
  placeholder?: string
}>()

const emit = defineEmits<{
  /** Emitido apenas em caso de sucesso com o nome do banco criado */
  created: [databaseName: string]
}>()

// ── Estado ────────────────────────────────────────────────────────────────────

const isOpen = defineModel<boolean>({ default: false })

const notify = useNotifier()
const databaseName = ref('')
const errorMessage = ref('')
const loading = ref(false)

// ── Computed ──────────────────────────────────────────────────────────────────

const DB_NAME_PATTERN = /^[a-zA-Z_][a-zA-Z0-9_-]*$/

const isValid = computed(
  () => databaseName.value.trim().length > 0 && DB_NAME_PATTERN.test(databaseName.value.trim()),
)

// ── Ações ─────────────────────────────────────────────────────────────────────

function close() {
  isOpen.value = false
}

async function submit() {
  if (!isValid.value || !props.connectionId) return

  loading.value = true
  errorMessage.value = ''

  try {
    const name = databaseName.value.trim()
    await connectionsApi.createDatabase(props.connectionId, name)
    notify(`Banco de dados "${name}" criado com sucesso`, 'success')
    emit('created', name)
    close()
  } catch (error: unknown) {
    const msg = error instanceof Error ? error.message : 'Erro ao criar banco de dados'
    errorMessage.value = msg
  } finally {
    loading.value = false
  }
}

// ── Reset ao fechar ───────────────────────────────────────────────────────────

watch(isOpen, (open) => {
  if (!open) {
    databaseName.value = ''
    errorMessage.value = ''
    loading.value = false
  }
})
</script>
