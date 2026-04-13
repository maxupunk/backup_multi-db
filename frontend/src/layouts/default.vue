<template>
  <v-app-bar :elevation="0" class="border-b" color="surface">
    <v-app-bar-nav-icon @click="toggleDrawer" />

    <v-app-bar-title class="font-weight-bold text-h6">
      {{ pageTitle }}
    </v-app-bar-title>

    <template #append>
      <v-btn icon variant="text" @click="toggleTheme">
        <v-icon :icon="isDark ? 'mdi-weather-night' : 'mdi-weather-sunny'" />
        <v-tooltip activator="parent" location="bottom">Alternar Tema</v-tooltip>
      </v-btn>

      <v-menu v-if="authStore.user" location="bottom end" min-width="240" offset="12">
        <template #activator="{ props }">
          <v-btn class="mr-2" icon variant="text" v-bind="props">
            <v-avatar color="primary" size="36">
              <span class="text-body-2 font-weight-bold">{{ userInitials }}</span>
            </v-avatar>
          </v-btn>
        </template>
        <v-list class="pa-2" rounded="lg">
          <v-list-item class="mb-2" :subtitle="authStore.user.email" :title="authStore.user.fullName || 'Usuário'">
            <template #prepend>
              <v-avatar color="primary" size="44">
                <span class="text-h6 font-weight-bold">{{ userInitials }}</span>
              </v-avatar>
            </template>
          </v-list-item>
          <v-divider class="mb-2" />
          <v-list-item base-color="error" prepend-icon="mdi-logout" rounded="lg" title="Sair" @click="handleLogout" />
        </v-list>
      </v-menu>
    </template>
  </v-app-bar>

  <v-navigation-drawer v-model="drawer" class="border-e" color="surface" :rail="rail" :temporary="!mdAndUp">
    <!-- Brand -->
    <v-list-item class="py-4" nav>
      <template #prepend>
        <v-avatar color="primary" rounded="lg" size="36">
          <v-icon icon="mdi-database-sync" size="20" />
        </v-avatar>
      </template>
      <v-list-item-title class="font-weight-bold text-subtitle-1">DB Backup</v-list-item-title>
      <v-list-item-subtitle class="text-caption">Manager v1.0</v-list-item-subtitle>
    </v-list-item>

    <v-divider />

    <!-- Navigation -->
    <v-list class="mt-2 px-2" density="compact" nav>
      <!-- Main section -->
      <v-list-subheader v-if="!rail">Principal</v-list-subheader>
      <v-list-item v-for="item in navItems" :key="item.to" class="mb-1" color="primary" :prepend-icon="item.icon"
        rounded="lg" :title="item.title" :to="item.to" />
      <!-- Docker section -->
      <v-divider class="my-2" />
      <v-list-subheader v-if="!rail">
        <span class="d-flex align-center ga-1">
          Docker
          <v-icon v-if="dockerUnavailable" color="warning" icon="mdi-alert" size="14" />
        </span>
      </v-list-subheader>
      <v-list-item v-for="sub in dockerSubItems" :key="sub.to" class="mb-1" color="primary" rounded="lg"
        :title="sub.title" :to="sub.to">
        <template #prepend>
          <v-badge v-if="dockerUnavailable && sub.to === '/docker'" color="warning" dot floating offset-x="2"
            offset-y="2">
            <v-icon>{{ sub.icon }}</v-icon>
          </v-badge>
          <v-icon v-else>{{ sub.icon }}</v-icon>
        </template>
      </v-list-item>

      <!-- System section -->
      <v-divider class="my-2" />
      <v-list-subheader v-if="!rail">Sistema</v-list-subheader>
      <v-list-item v-for="item in systemNavItems" :key="item.to" class="mb-1" color="primary" :prepend-icon="item.icon"
        rounded="lg" :title="item.title" :to="item.to" />


    </v-list>
  </v-navigation-drawer>

  <v-main>
    <v-container class="pa-4 pa-sm-6" fluid>
      <router-view />
    </v-container>
  </v-main>

  <NotificationToast />
  <RestoreProgressOverlay />
</template>

<script lang="ts" setup>
import { computed, onMounted, provide, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useDisplay, useTheme } from 'vuetify'
import { useAuthStore } from '@/stores/auth'
import { useNotificationStore } from '@/stores/notification'
import NotificationToast from '@/components/NotificationToast.vue'
import RestoreProgressOverlay from '@/components/RestoreProgressOverlay.vue'
import { dockerContainersApi } from '@/services/dockerService'

const theme = useTheme()
const { mdAndUp } = useDisplay()
const authStore = useAuthStore()
const notificationStore = useNotificationStore()
const router = useRouter()
const route = useRoute()

const navItems = [
  { title: 'Dashboard', icon: 'mdi-view-dashboard-outline', to: '/' },
  { title: 'Conexões', icon: 'mdi-database-outline', to: '/connections' },
  { title: 'Backups', icon: 'mdi-backup-restore', to: '/backups' },
  { title: 'Armazenamentos', icon: 'mdi-bucket-outline', to: '/storages' },
]

const systemNavItems = computed(() => [
  { title: 'Auditoria', icon: 'mdi-history', to: '/audit' },
  ...(authStore.user?.isAdmin ? [{ title: 'Usuários', icon: 'mdi-account-group-outline', to: '/users' }] : []),
  { title: 'Configurações', icon: 'mdi-cog-outline', to: '/settings' },
])

const dockerSubItems = [
  { title: 'Visão Geral', icon: 'mdi-docker', to: '/docker' },
  { title: 'Containers', icon: 'mdi-cube-outline', to: '/docker/containers' },
  { title: 'Volumes', icon: 'mdi-database-outline', to: '/docker/volumes' },
  { title: 'Redes', icon: 'mdi-graph-outline', to: '/docker/networks' },
  { title: 'Imagens', icon: 'mdi-layers-outline', to: '/docker/images' },
]

const dockerUnavailable = ref(false)

async function checkDockerAvailability() {
  try {
    await dockerContainersApi.getGroups()
    dockerUnavailable.value = false
  } catch (err) {
    dockerUnavailable.value = (err as Error).message === 'DOCKER_UNAVAILABLE'
  }
}

const drawer = ref(mdAndUp.value)
const rail = ref(false)

// 3-state cycle on desktop: full → icon-only → hidden
// On mobile: simple toggle show/hide

const isDark = computed(() => theme.global.current.value.dark)

const pageTitle = computed(() => {
  const allItems = [
    ...navItems,
    ...systemNavItems.value,
    ...dockerSubItems,
  ]
  const sortedItems = [...allItems].sort((a, b) => b.to.length - a.to.length)
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

function toggleDrawer() {
  if (mdAndUp.value) {
    if (!drawer.value) {
      // Hidden → open full
      drawer.value = true
      rail.value = false
    } else if (!rail.value) {
      // Full → icon-only
      rail.value = true
    } else {
      // Icon-only → hidden
      drawer.value = false
      rail.value = false
    }
  } else {
    drawer.value = !drawer.value
  }
}

function toggleTheme() {
  theme.change(isDark.value ? 'light' : 'dark')
}

async function handleLogout() {
  await authStore.logout()
  router.push('/login')
}

onMounted(() => {
  if (authStore.isAuthenticated) checkDockerAvailability()
})



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
