<template>
  <v-table density="compact" fixed-header>
    <thead>
      <tr>
        <th>Rede</th>
        <th>IP</th>
        <th>Gateway</th>
        <th>Aliases</th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="net in networks" :key="net.networkId">
        <td class="text-caption font-weight-medium">{{ net.networkName }}</td>
        <td class="text-caption text-monospace">{{ net.ipAddress || '—' }}</td>
        <td class="text-caption text-monospace">{{ net.gateway || '—' }}</td>
        <td class="text-caption">
          <span v-if="net.aliases?.length">{{ net.aliases.join(', ') }}</span>
          <span v-else class="text-medium-emphasis">—</span>
        </td>
      </tr>
      <tr v-if="networks.length === 0">
        <td class="text-caption text-medium-emphasis text-center py-4" colspan="4">
          Nenhuma rede conectada.
        </td>
      </tr>
    </tbody>
  </v-table>
</template>

<script lang="ts" setup>
import type { DockerNetworkEndpoint } from '@/types/api'
defineProps<{ networks: DockerNetworkEndpoint[] }>()
</script>
