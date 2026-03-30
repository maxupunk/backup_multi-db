<template>
  <v-btn
    :color="status === 'success' ? 'success' : status === 'error' ? 'error' : 'primary'"
    :loading="testing"
    :prepend-icon="statusIcon"
    variant="tonal"
    @click="runTest"
  >
    {{ statusLabel }}
  </v-btn>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { ApiError, storagesApi } from '@/services/api'

const props = defineProps<{
  storageId: number
}>()

const emit = defineEmits<{
  result: [success: boolean, latencyMs?: number]
}>()

const testing = ref(false)
const status = ref<'idle' | 'success' | 'error'>('idle')
const latency = ref<number | null>(null)

const statusIcon = computed(() => {
  if (status.value === 'success') return 'mdi-check-circle'
  if (status.value === 'error') return 'mdi-alert-circle'
  return 'mdi-connection'
})

const statusLabel = computed(() => {
  if (testing.value) return 'Testando...'
  if (status.value === 'success') return `Conectado (${latency.value}ms)`
  if (status.value === 'error') return 'Falhou'
  return 'Testar Conexão'
})

async function runTest () {
  testing.value = true
  status.value = 'idle'

  try {
    const result = await storagesApi.test(props.storageId)
    latency.value = result.data?.latencyMs ?? 0
    status.value = 'success'
    emit('result', true, latency.value)
  } catch (error) {
    status.value = 'error'
    const msg = error instanceof ApiError ? error.message : 'Erro ao testar'
    console.error(msg)
    emit('result', false)
  } finally {
    testing.value = false
  }
}
</script>
