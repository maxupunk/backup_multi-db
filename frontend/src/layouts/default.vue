<template>
  <v-navigation-drawer v-model="drawer" :rail="rail" permanent class="bg-surface">
    <!-- Logo -->
    <v-list-item class="pa-4" :class="{ 'justify-center': rail }">
      <template #prepend>
        <v-avatar color="primary" size="40" class="elevation-2">
          <v-icon icon="mdi-database-sync" size="24" />
        </v-avatar>
      </template>

      <v-list-item-title v-if="!rail" class="text-h6 font-weight-bold">
        DB Backup
      </v-list-item-title>
      <v-list-item-subtitle v-if="!rail" class="text-caption">
        Manager v1.0
      </v-list-item-subtitle>
    </v-list-item>

    <v-divider class="my-2" />

    <!-- Navigation -->
    <v-list density="compact" nav>
      <v-list-item v-for="item in navItems" :key="item.to" :to="item.to" :title="item.title" :prepend-icon="item.icon"
        rounded="lg" class="mb-1" color="primary" />
    </v-list>

    <template #append>
      <v-divider class="mb-2" />

      <!-- Theme Toggle -->
      <v-list-item :title="rail ? '' : 'Tema'" :prepend-icon="isDark ? 'mdi-weather-night' : 'mdi-weather-sunny'"
        @click="toggleTheme" rounded="lg" class="mx-2 mb-2" />

      <!-- Rail Toggle -->
      <v-list-item :title="rail ? '' : 'Recolher'" :prepend-icon="rail ? 'mdi-chevron-right' : 'mdi-chevron-left'"
        @click="rail = !rail" rounded="lg" class="mx-2 mb-2" />
    </template>
  </v-navigation-drawer>

  <v-main class="bg-background">
    <v-container fluid class="pa-6">
      <router-view />
    </v-container>
  </v-main>

  <!-- Snackbar global para notificações -->
  <v-snackbar v-model="snackbar.show" :color="snackbar.color" :timeout="snackbar.timeout" location="bottom right"
    rounded="lg">
    <div class="d-flex align-center">
      <v-icon :icon="snackbar.icon" class="mr-2" />
      {{ snackbar.message }}
    </div>

    <template #actions>
      <v-btn variant="text" icon="mdi-close" size="small" @click="snackbar.show = false" />
    </template>
  </v-snackbar>
</template>

<script lang="ts" setup>
import { ref, computed, provide, reactive } from 'vue'
import { useTheme } from 'vuetify'

const theme = useTheme()
const drawer = ref(true)
const rail = ref(false)

const isDark = computed(() => theme.global.current.value.dark)

function toggleTheme() {
  theme.global.name.value = isDark.value ? 'light' : 'dark'
}

const navItems = [
  { title: 'Dashboard', icon: 'mdi-view-dashboard', to: '/' },
  { title: 'Conexões', icon: 'mdi-database', to: '/connections' },
  { title: 'Backups', icon: 'mdi-backup-restore', to: '/backups' },
  { title: 'Auditoria', icon: 'mdi-history', to: '/audit' },
  { title: 'Configurações', icon: 'mdi-cog', to: '/settings' },
]

// Sistema de notificações global
const snackbar = reactive({
  show: false,
  message: '',
  color: 'success',
  icon: 'mdi-check-circle',
  timeout: 4000,
})

function showNotification(
  message: string,
  type: 'success' | 'error' | 'warning' | 'info' = 'success'
) {
  const icons = {
    success: 'mdi-check-circle',
    error: 'mdi-alert-circle',
    warning: 'mdi-alert',
    info: 'mdi-information',
  }

  snackbar.message = message
  snackbar.color = type
  snackbar.icon = icons[type]
  snackbar.show = true
}

// Prover função de notificação globalmente
provide('showNotification', showNotification)
</script>

<style scoped>
.v-navigation-drawer {
  border-right: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
}
</style>
