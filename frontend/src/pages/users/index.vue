<template>
    <div>
        <!-- Header -->
        <div class="d-flex align-center justify-space-between mb-6">
            <div>
                <h1 class="text-h4 font-weight-bold mb-1">Usuários</h1>
                <p class="text-body-2 text-medium-emphasis">
                    Gerencie o acesso dos usuários ao sistema
                </p>
            </div>
        </div>

        <!-- Filters (Optional, can be added later) -->

        <!-- Users List -->
        <v-card>
            <v-data-table :headers="headers" :items="users" :loading="loading" :items-per-page="10" class="elevation-0">

                <!-- Status -->
                <template #item.isActive="{ item }">
                    <v-chip :color="item.isActive ? 'success' : 'warning'" size="small" label>
                        {{ item.isActive ? 'Ativo' : 'Pendente/Inativo' }}
                    </v-chip>
                </template>

                <!-- CreatedAt -->
                <template #item.createdAt="{ item }">
                    {{ formatDate(item.createdAt) }}
                </template>

                <!-- Actions -->
                <template #item.actions="{ item }">
                    <v-tooltip location="top" :text="item.isActive ? 'Desativar acesso' : 'Aprovar/Ativar acesso'">
                        <template v-slot:activator="{ props }">
                            <v-btn v-bind="props" :icon="item.isActive ? 'mdi-account-off' : 'mdi-account-check'"
                                size="small" variant="text" :color="item.isActive ? 'error' : 'success'"
                                :loading="actionLoading[item.id]" @click="toggleStatus(item)">
                            </v-btn>
                        </template>
                    </v-tooltip>
                </template>

                <!-- No data -->
                <template #no-data>
                    <div class="text-center py-8">
                        <v-icon icon="mdi-account-off" size="64" color="grey" class="mb-4" />
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
import { ref, reactive, onMounted, inject } from 'vue'
import { usersApi } from '@/services/api'
import type { User } from '@/types/auth'

const showNotification = inject<(msg: string, type: string) => void>('showNotification')

const loading = ref(false)
const users = ref<User[]>([])
const actionLoading = reactive<Record<number, boolean>>({})

const headers = [
    { title: 'Nome', key: 'fullName', sortable: true },
    { title: 'E-mail', key: 'email', sortable: true },
    { title: 'Status', key: 'isActive', sortable: true },
    { title: 'Data Cadastro', key: 'createdAt', sortable: true },
    { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
]

async function loadUsers() {
    loading.value = true
    try {
        const response = await usersApi.list()
        // Check structure of pagination response
        users.value = response.data || []
        // If paginated, it might be response.data.data
        if ((response as any).data && Array.isArray((response as any).data)) {
            users.value = (response as any).data
        } else if (Array.isArray(response)) {
            users.value = response
        } else if (response.data && Array.isArray(response.data)) {
            users.value = response.data
        }

    } catch (error) {
        console.error('Erro ao carregar usuários:', error)
        showNotification?.('Erro ao carregar usuários', 'error')
    } finally {
        loading.value = false
    }
}

async function toggleStatus(user: User) {
    actionLoading[user.id] = true
    try {
        await usersApi.toggleStatus(user.id)
        showNotification?.(`Status de ${user.fullName} alterado com sucesso!`, 'success')

        // Optimistic update or reload
        user.isActive = !user.isActive
    } catch (error) {
        console.error(error)
        showNotification?.('Erro ao alterar status do usuário', 'error')
    } finally {
        actionLoading[user.id] = false
    }
}

function formatDate(dateString?: string): string {
    if (!dateString) return '-'
    const date = new Date(dateString)
    return date.toLocaleString('pt-BR', {
        day: '2-digit',
        month: '2-digit',
        year: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    })
}

onMounted(() => {
    loadUsers()
})
</script>
