<template>
  <v-app-bar class="bg-surface" :elevation="0">
    <v-app-bar-nav-icon v-if="!mdAndUp" @click="drawer = !drawer" />

    <v-app-bar-title class="font-weight-bold">
      DB Backup
    </v-app-bar-title>

    <template #append>
      <v-btn :aria-label="isDark ? 'Ativar tema claro' : 'Ativar tema escuro'" icon @click="toggleTheme">
        <v-icon :icon="isDark ? 'mdi-weather-night' : 'mdi-weather-sunny'" />
      </v-btn>

      <v-btn v-if="authStore.user" aria-label="Sair" color="error" icon @click="handleLogout">
        <v-icon icon="mdi-logout" />
      </v-btn>
    </template>
  </v-app-bar>

  <v-navigation-drawer v-model="drawer" class="bg-surface" :permanent="mdAndUp" :rail="mdAndUp && rail"
    :temporary="!mdAndUp">
    <!-- Logo -->
    <v-list-item class="pa-4" :class="{ 'justify-center': rail }">
      <template #prepend>
        <v-avatar class="elevation-2" color="primary" size="40">
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
      <v-list-item v-for="item in navItems" :key="item.to" class="mb-1" color="primary" :prepend-icon="item.icon"
        rounded="lg" :title="item.title" :to="item.to" />
    </v-list>

    <template #append>
      <v-divider class="mb-2" />

      <!-- User Profile -->
      <v-list-item v-if="authStore.user" class="mx-2 mb-2" prepend-icon="mdi-account-circle" rounded="lg"
        :subtitle="rail ? '' : authStore.user.email" :title="rail ? '' : (authStore.user.fullName || 'Usuário')">
        <template v-if="!rail" #append>
          <v-btn color="error" icon="mdi-logout" size="small" variant="text" @click="handleLogout">
            <v-icon icon="mdi-logout" />
            <v-tooltip activator="parent" location="top">Sair</v-tooltip>
          </v-btn>
        </template>
      </v-list-item>

      <v-divider v-if="authStore.user" class="mb-2" />

      <!-- Theme Toggle -->
      <v-list-item class="mx-2 mb-2" :prepend-icon="isDark ? 'mdi-weather-night' : 'mdi-weather-sunny'" rounded="lg"
        :title="rail ? '' : 'Tema'" @click="toggleTheme" />

      <!-- Rail Toggle -->
      <v-list-item v-if="mdAndUp" class="mx-2 mb-2" :prepend-icon="rail ? 'mdi-chevron-right' : 'mdi-chevron-left'"
        rounded="lg" :title="rail ? '' : 'Recolher'" @click="rail = !rail" />
    </template>
  </v-navigation-drawer>

  <v-main class="bg-background">
    <v-container :class="mdAndUp ? 'pa-6' : 'pa-3'" fluid>
      <router-view />
    </v-container>
  </v-main>

  <NotificationToast />
</template>

<script lang="ts" setup>
import { computed, provide, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useDisplay, useTheme } from 'vuetify'
import { useAuthStore } from '@/stores/auth'
import { useNotificationStore } from '@/stores/notification'
import NotificationToast from '@/components/NotificationToast.vue'

const theme = useTheme()
const { mdAndUp } = useDisplay()
const authStore = useAuthStore()
const notificationStore = useNotificationStore()
const router = useRouter()
const drawer = ref(mdAndUp.value)
const rail = ref(false)

const isDark = computed(() => theme.global.current.value.dark)

watch(
  mdAndUp,
  isDesktop => {
    drawer.value = isDesktop
    if (!isDesktop) rail.value = false
  },
  { immediate: false },
)

function toggleTheme() {
  theme.change(isDark.value ? 'light' : 'dark')
}

async function handleLogout() {
  await authStore.logout()
  router.push('/login')
}

const navItems = [
  { title: 'Dashboard', icon: 'mdi-view-dashboard', to: '/' },
  { title: 'Conexões', icon: 'mdi-database', to: '/connections' },
  { title: 'Backups', icon: 'mdi-backup-restore', to: '/backups' },
  { title: 'Auditoria', icon: 'mdi-history', to: '/audit' },
  { title: 'Usuários', icon: 'mdi-account-group', to: '/users' },
  { title: 'Configurações', icon: 'mdi-cog', to: '/settings' },
]

function showNotification(
  message: string,
  type: 'success' | 'error' | 'warning' | 'info' = 'success',
) {
  notificationStore.add({
    type,
    category: 'system',
    title: type.charAt(0).toUpperCase() + type.slice(1),
    message,
  } as any)
}

// Prover função de notificação globalmente
provide('showNotification', showNotification)
</script>

<style scoped>
.v-navigation-drawer {
  border-right: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
}
</style>
