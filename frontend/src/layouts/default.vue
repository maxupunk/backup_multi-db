<template>
  <v-app-bar class="bg-surface border-b" :elevation="0">
    <v-app-bar-nav-icon v-if="!mdAndUp" @click="drawer = !drawer" />

    <v-app-bar-title class="font-weight-bold text-h6">
      {{ pageTitle }}
    </v-app-bar-title>

    <template #append>
      <v-btn class="mr-2" :icon="isDark ? 'mdi-weather-night' : 'mdi-weather-sunny'" variant="text"
        @click="toggleTheme">
        <v-icon :icon="isDark ? 'mdi-weather-night' : 'mdi-weather-sunny'" />
        <v-tooltip activator="parent" location="bottom">Alternar Tema</v-tooltip>
      </v-btn>

      <v-menu v-if="authStore.user" location="bottom end" min-width="230" offset="10">
        <template #activator="{ props }">
          <v-btn class="mr-2" icon v-bind="props">
            <v-avatar color="primary" size="32">
              <span class="text-subtitle-2 font-weight-bold">{{ userInitials }}</span>
            </v-avatar>
          </v-btn>
        </template>
        <v-list class="pa-2" rounded="lg">
          <v-list-item class="mb-2" :subtitle="authStore.user.email" :title="authStore.user.fullName || 'Usuário'">
            <template #prepend>
              <v-avatar color="primary" size="40">
                <span class="text-h6">{{ userInitials }}</span>
              </v-avatar>
            </template>
          </v-list-item>
          <v-divider class="mb-2" />
          <v-list-item color="error" prepend-icon="mdi-logout" rounded="lg" title="Sair" @click="handleLogout" />
        </v-list>
      </v-menu>
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
import { useRoute, useRouter } from 'vue-router'
import { useDisplay, useTheme } from 'vuetify'
import { useAuthStore } from '@/stores/auth'
import { useNotificationStore } from '@/stores/notification'
import NotificationToast from '@/components/NotificationToast.vue'

const theme = useTheme()
const { mdAndUp } = useDisplay()
const authStore = useAuthStore()
const notificationStore = useNotificationStore()
const router = useRouter()
const route = useRoute()

const navItems = [
  { title: 'Dashboard', icon: 'mdi-view-dashboard', to: '/' },
  { title: 'Conexões', icon: 'mdi-database', to: '/connections' },
  { title: 'Backups', icon: 'mdi-backup-restore', to: '/backups' },
  { title: 'Auditoria', icon: 'mdi-history', to: '/audit' },
  { title: 'Usuários', icon: 'mdi-account-group', to: '/users' },
  { title: 'Configurações', icon: 'mdi-cog', to: '/settings' },
]

const drawer = ref(mdAndUp.value)
const rail = ref(false)

const isDark = computed(() => theme.global.current.value.dark)

const pageTitle = computed(() => {
  const sortedItems = [...navItems].sort((a, b) => b.to.length - a.to.length)
  const item = sortedItems.find(i =>
    i.to === '/' ? route.path === '/' : route.path.startsWith(i.to)
  )
  return item ? item.title : ''
})

const userInitials = computed(() => {
  const name = authStore.user?.fullName || 'Usuário'
  return name
    .split(' ')
    .map((n: string) => n[0])
    .join('')
    .toUpperCase()
    .slice(0, 2)
})

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
