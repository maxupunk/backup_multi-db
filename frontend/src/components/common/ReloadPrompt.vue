<template>
    <v-snackbar v-model="needRefresh" location="bottom right" timeout="-1" color="info" variant="tonal">
        <div class="d-flex flex-column">
            <span class="mb-2">Uma nova versão está disponível.</span>
            <div class="d-flex justify-end">
                <v-btn variant="text" size="small" class="mr-2" @click="close">
                    Depois
                </v-btn>
                <v-btn color="primary" size="small" @click="updateServiceWorker()">
                    Atualizar
                </v-btn>
            </div>
        </div>
    </v-snackbar>

    <v-snackbar v-model="offlineReady" location="bottom right" :timeout="3000" color="success">
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

async function close() {
    offlineReady.value = false
    needRefresh.value = false
}
</script>
