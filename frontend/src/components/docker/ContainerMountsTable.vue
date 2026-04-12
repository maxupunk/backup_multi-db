<template>
  <v-table density="compact" fixed-header>
    <thead>
      <tr>
        <th>Tipo</th>
        <th>Origem</th>
        <th>Destino (Container)</th>
        <th>Modo</th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="(m, i) in mounts" :key="i">
        <td><v-chip label size="x-small" variant="tonal">{{ m.type }}</v-chip></td>
        <td class="text-caption text-monospace">{{ m.source || m.name || '—' }}</td>
        <td class="text-caption text-monospace">{{ m.destination }}</td>
        <td>
          <v-chip
            :color="m.rw ? 'success' : 'warning'"
            label
            size="x-small"
            variant="tonal"
          >
            {{ m.rw ? 'RW' : 'RO' }}
          </v-chip>
        </td>
      </tr>
      <tr v-if="mounts.length === 0">
        <td class="text-caption text-medium-emphasis text-center py-4" colspan="4">
          Nenhum volume montado.
        </td>
      </tr>
    </tbody>
  </v-table>
</template>

<script lang="ts" setup>
import type { DockerMount } from '@/types/api'
defineProps<{ mounts: DockerMount[] }>()
</script>
