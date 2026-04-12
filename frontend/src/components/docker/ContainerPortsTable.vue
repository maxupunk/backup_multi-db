<template>
  <v-table density="compact" fixed-header>
    <thead>
      <tr>
        <th>Host</th>
        <th>Container</th>
        <th>Protocolo</th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="(port, i) in ports" :key="i">
        <td class="text-caption text-monospace">
          {{ port.IP ? `${port.IP}:` : '' }}{{ port.PublicPort ?? '—' }}
        </td>
        <td class="text-caption text-monospace">{{ port.PrivatePort }}</td>
        <td>
          <v-chip label size="x-small" variant="tonal">{{ port.Type }}</v-chip>
        </td>
      </tr>
      <tr v-if="ports.length === 0">
        <td class="text-caption text-medium-emphasis text-center py-4" colspan="3">
          Nenhuma porta exposta.
        </td>
      </tr>
    </tbody>
  </v-table>
</template>

<script lang="ts" setup>
import type { DockerContainerPort } from '@/types/api'
defineProps<{ ports: DockerContainerPort[] }>()
</script>
