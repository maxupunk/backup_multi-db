<template>
  <div>
    <div class="d-flex align-center mb-2">
      <v-text-field
        v-model="search"
        clearable
        density="compact"
        hide-details
        placeholder="Filtrar variáveis..."
        prepend-inner-icon="mdi-magnify"
        variant="outlined"
      />
      <v-btn
        class="ml-2"
        density="compact"
        :icon="showValues ? 'mdi-eye-off-outline' : 'mdi-eye-outline'"
        variant="text"
        @click="showValues = !showValues"
      >
        <v-tooltip activator="parent" location="top">
          {{ showValues ? 'Ocultar valores' : 'Mostrar valores' }}
        </v-tooltip>
      </v-btn>
    </div>

    <v-table density="compact" fixed-header>
      <thead>
        <tr>
          <th class="text-left">Variável</th>
          <th class="text-left">Valor</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="entry in filtered" :key="entry.key">
          <td class="text-caption font-weight-medium">{{ entry.key }}</td>
          <td class="text-caption">
            <span v-if="showValues">{{ entry.value }}</span>
            <span v-else class="text-medium-emphasis">••••••••</span>
          </td>
        </tr>
        <tr v-if="filtered.length === 0">
          <td class="text-caption text-medium-emphasis text-center py-4" colspan="2">
            Nenhuma variável encontrada.
          </td>
        </tr>
      </tbody>
    </v-table>
  </div>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'

const props = defineProps<{ env: string[] }>()

const search = ref('')
const showValues = ref(false)

const parsed = computed(() =>
  props.env.map((line) => {
    const eqIdx = line.indexOf('=')
    if (eqIdx < 0) return { key: line, value: '' }
    return { key: line.slice(0, eqIdx), value: line.slice(eqIdx + 1) }
  })
)

const filtered = computed(() => {
  if (!search.value) return parsed.value
  const q = search.value.toLowerCase()
  return parsed.value.filter((e) => e.key.toLowerCase().includes(q))
})
</script>
