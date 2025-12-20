<template>
  <v-snackbar
    v-model="needRefresh"
    color="info"
    location="bottom right"
    timeout="-1"
    variant="tonal"
  >
    <div class="d-flex flex-column">
      <span class="mb-2">Uma nova versão está disponível.</span>
      <div class="d-flex justify-end">
        <v-btn class="mr-2" size="small" variant="text" @click="close">
          Depois
        </v-btn>
        <v-btn color="primary" size="small" @click="updateServiceWorker()">
          Atualizar
        </v-btn>
      </div>
    </div>
  </v-snackbar>

  <v-snackbar v-model="offlineReady" color="success" location="bottom right" :timeout="3000">
    Aplicativo pronto para uso offline
  </v-snackbar>
</template>

<script setup lang="ts">
  import { useRegisterSW } from 'virtual:pwa-register/vue'

  const {
    offlineReady,
    needRefresh,
    updateServiceWorker,
  } = useRegisterSW()

  async function close () {
    offlineReady.value = false
    needRefresh.value = false
  }
</script>
