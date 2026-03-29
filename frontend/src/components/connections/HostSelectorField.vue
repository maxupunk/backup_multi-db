<template>
  <div>
    <v-combobox
      v-model="hostFieldValue"
      item-title="title"
      item-value="value"
      :items="hostItems"
      label="Host *"
      placeholder="Digite manualmente ou selecione uma sugestão Docker"
      prepend-inner-icon="mdi-server"
      :loading="loading"
      :rules="[requiredRule]"
      @update:model-value="handleHostUpdated"
    />

    <v-alert v-if="selectedSuggestion?.connectivityWarning" class="mt-2" color="warning" density="compact" variant="tonal">
      {{ selectedSuggestion.connectivityWarning }}
    </v-alert>

    <v-alert v-if="unavailableReason" class="mt-2" color="info" density="compact" variant="tonal">
      {{ unavailableReason }}
    </v-alert>
  </div>
</template>

<script lang="ts" setup>
import type { DockerHostSuggestion } from '@/types/api'
import { computed, ref, watch } from 'vue'

interface HostItem {
  title: string
  value: string
}

const props = defineProps<{
  modelValue: string
  suggestions: DockerHostSuggestion[]
  loading: boolean
  unavailableReason: string
  requiredRule: (v: string) => true | string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
  'suggestion-selected': [suggestion: DockerHostSuggestion | null]
}>()

const hostFieldValue = ref('')
const selectedSuggestion = ref<DockerHostSuggestion | null>(null)

const suggestionsByKey = computed(() => {
  const map = new Map<string, DockerHostSuggestion>()
  for (const suggestion of props.suggestions) {
    map.set(buildSuggestionKey(suggestion), suggestion)
  }
  return map
})

const hostItems = computed<HostItem[]>(() => {
  return props.suggestions.map((suggestion) => ({
    title: `${suggestion.containerName} • ${suggestion.suggestedHost} • ${hostResolutionLabel(suggestion)}`,
    value: buildSuggestionKey(suggestion),
  }))
})

function buildSuggestionKey(suggestion: DockerHostSuggestion): string {
  return `${suggestion.containerId}:${suggestion.suggestedHost}`
}

function hostResolutionLabel(suggestion: DockerHostSuggestion): string {
  if (suggestion.hostResolutionSource === 'docker_dns') {
    return 'Docker DNS (mesma rede)'
  }

  if (suggestion.hostResolutionSource === 'host_ip') {
    return 'IP do host (rede diferente)'
  }

  return 'Fallback'
}

function handleHostUpdated(value: string | { value?: string } | null) {
  const rawValue =
    typeof value === 'string' ? value : value && typeof value.value === 'string' ? value.value : ''

  hostFieldValue.value = rawValue

  const suggestion = suggestionsByKey.value.get(rawValue)
  selectedSuggestion.value = suggestion ?? null
  emit('suggestion-selected', selectedSuggestion.value)

  if (suggestion) {
    emit('update:modelValue', suggestion.suggestedHost)
    return
  }

  emit('update:modelValue', rawValue)
}

function syncFromModel() {
  const hostMatches = props.suggestions.filter((suggestion) => suggestion.suggestedHost === props.modelValue)
  const singleMatch = hostMatches[0]

  if (hostMatches.length === 1 && singleMatch !== undefined) {
    selectedSuggestion.value = singleMatch
    hostFieldValue.value = buildSuggestionKey(singleMatch)
    emit('suggestion-selected', singleMatch)
    return
  }

  selectedSuggestion.value = null
  hostFieldValue.value = props.modelValue
  emit('suggestion-selected', null)
}

watch(
  () => [props.modelValue, props.suggestions],
  () => {
    syncFromModel()
  },
  { deep: true, immediate: true }
)
</script>
