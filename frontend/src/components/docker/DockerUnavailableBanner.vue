<template>
  <v-alert border="start" class="mb-4" color="warning" icon="mdi-docker" variant="tonal">
    <v-alert-title>Docker indisponível</v-alert-title>
    {{ reason || 'O socket do Docker (/var/run/docker.sock) não está acessível. Verifique se o container tem acesso ao socket.' }}
  </v-alert>
</template>

<script lang="ts" setup>
defineProps<{ reason?: string | null }>()
</script>
