<template>
  <div>
    <!-- Header -->
    <v-row align="center" class="mb-6">
      <v-col cols="12">
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">Usuários</h1>
        <p class="text-body-2 text-medium-emphasis">
          Gerencie o acesso dos usuários ao sistema
        </p>
      </v-col>
    </v-row>

    <!-- Filters (Optional, can be added later) -->

    <!-- Users List -->
    <v-card>
      <v-data-table
        class="elevation-0"
        :headers="tableHeaders"
        :items="users"
        :items-per-page="mdAndUp ? 10 : 5"
        :loading="loading"
        :mobile="!mdAndUp"
      >

        <template #item.fullName="{ item }">
          <template v-if="!mdAndUp">
            <div class="font-weight-medium">{{ item.fullName }}</div>
            <div class="text-caption text-medium-emphasis">{{ item.email }}</div>
          </template>
          <span v-else>{{ item.fullName }}</span>
        </template>

        <!-- Status -->
        <template #item.isActive="{ item }">
          <v-chip :color="item.isActive ? 'success' : 'warning'" label size="small">
            {{ item.isActive ? 'Ativo' : 'Pendente/Inativo' }}
          </v-chip>
        </template>

        <!-- CreatedAt -->
        <template #item.createdAt="{ item }">
          {{ formatDate(item.createdAt) }}
        </template>

        <!-- Actions -->
        <template #item.actions="{ item }">
          <template v-if="mdAndUp">
            <v-tooltip location="top" :text="item.isActive ? 'Desativar acesso' : 'Aprovar/Ativar acesso'">
              <template #activator="{ props }">
                <v-btn
                  v-bind="props"
                  :color="item.isActive ? 'error' : 'success'"
                  :icon="item.isActive ? 'mdi-account-off' : 'mdi-account-check'"
                  :loading="actionLoading[item.id]"
                  size="small"
                  variant="text"
                  @click="toggleStatus(item)"
                />
              </template>
            </v-tooltip>
          </template>

          <v-btn
            v-else
            :aria-label="item.isActive ? 'Desativar acesso' : 'Aprovar e ativar acesso'"
            :color="item.isActive ? 'error' : 'success'"
            :icon="item.isActive ? 'mdi-account-off' : 'mdi-account-check'"
            :loading="actionLoading[item.id]"
            variant="text"
            @click="toggleStatus(item)"
          />
        </template>

        <!-- No data -->
        <template #no-data>
          <div class="text-center py-8">
            <v-icon class="mb-4" color="grey" icon="mdi-account-off" size="64" />
            <p class="text-h6 text-medium-emphasis mb-2">
              Nenhum usuário encontrado
            </p>
          </div>
        </template>
      </v-data-table>
    </v-card>
  </div>
</template>

<script lang="ts" setup>
  import type { User } from '@/types/auth'
  import { computed, onMounted, reactive, ref } from 'vue'
  import { useDisplay } from 'vuetify'
  import { usersApi } from '@/services/api'
  import { useNotifier } from '@/composables/useNotifier'
  import { formatDateTimePtBR as formatDate } from '@/utils/format'

  const notify = useNotifier()
  const { mdAndUp } = useDisplay()

  const loading = ref(false)
  const users = ref<User[]>([])
  const actionLoading = reactive<Record<number, boolean>>({})

  const desktopHeaders = [
    { title: 'Nome', key: 'fullName', sortable: true },
    { title: 'E-mail', key: 'email', sortable: true },
    { title: 'Status', key: 'isActive', sortable: true },
    { title: 'Data Cadastro', key: 'createdAt', sortable: true },
    { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
  ]

  const mobileHeaders = [
    { title: 'Usuário', key: 'fullName', sortable: true },
    { title: 'Status', key: 'isActive', sortable: true },
    { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
  ]

  const tableHeaders = computed(() => (mdAndUp.value ? desktopHeaders : mobileHeaders))

  async function loadUsers () {
    loading.value = true
    try {
      const response = await usersApi.list()
      users.value = response.data?.data ?? []
    } catch (error) {
      console.error('Erro ao carregar usuários:', error)
      notify('Erro ao carregar usuários', 'error')
    } finally {
      loading.value = false
    }
  }

  async function toggleStatus (user: User) {
    actionLoading[user.id] = true
    try {
      await usersApi.toggleStatus(user.id)
      notify(`Status de ${user.fullName} alterado com sucesso!`, 'success')

      // Optimistic update or reload
      user.isActive = !user.isActive
    } catch (error) {
      console.error(error)
      notify('Erro ao alterar status do usuário', 'error')
    } finally {
      actionLoading[user.id] = false
    }
  }

  onMounted(() => {
    loadUsers()
  })
</script>
