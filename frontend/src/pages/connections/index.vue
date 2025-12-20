<template>
    <div>
        <!-- Header -->
        <div class="d-flex align-center justify-space-between mb-6">
            <div>
                <h1 class="text-h4 font-weight-bold mb-1">Conexões</h1>
                <p class="text-body-2 text-medium-emphasis">
                    Gerencie suas conexões de banco de dados
                </p>
            </div>

            <v-btn color="primary" prepend-icon="mdi-plus" to="/connections/new">
                Nova Conexão
            </v-btn>
        </div>

        <!-- Filters -->
        <v-card class="mb-6">
            <v-card-text>
                <v-row>
                    <v-col cols="12" sm="4">
                        <v-text-field v-model="filters.search" label="Buscar" prepend-inner-icon="mdi-magnify" clearable
                            hide-details @update:model-value="debouncedLoad" />
                    </v-col>

                    <v-col cols="12" sm="4">
                        <v-select v-model="filters.type" label="Tipo de Banco" :items="databaseTypes" clearable
                            hide-details @update:model-value="loadConnections" />
                    </v-col>

                    <v-col cols="12" sm="4">
                        <v-select v-model="filters.status" label="Status" :items="statusOptions" clearable hide-details
                            @update:model-value="loadConnections" />
                    </v-col>
                </v-row>
            </v-card-text>
        </v-card>

        <!-- Connections List -->
        <v-card>
            <v-data-table :headers="headers" :items="connections" :loading="loading" :items-per-page="10"
                class="elevation-0">
                <!-- Type -->
                <template #item.type="{ item }">
                    <v-chip :color="getDatabaseColor(item.type)" size="small" label>
                        <v-icon :icon="getDatabaseIcon(item.type)" size="16" class="mr-1" />
                        {{ item.type.toUpperCase() }}
                    </v-chip>
                </template>

                <!-- Host -->
                <template #item.host="{ item }">
                    <div>
                        <span class="font-weight-medium">{{ item.host }}</span>
                        <span class="text-medium-emphasis">:{{ item.port }}</span>
                    </div>
                    <div class="text-caption text-medium-emphasis">
                        {{ item.database }}
                    </div>
                </template>

                <!-- Status -->
                <template #item.status="{ item }">
                    <v-chip :color="getStatusColor(item.status)" size="small" label>
                        {{ getStatusLabel(item.status) }}
                    </v-chip>
                </template>

                <!-- Schedule -->
                <template #item.schedule="{ item }">
                    <template v-if="item.scheduleEnabled && item.scheduleFrequency">
                        <v-chip color="success" size="small" variant="tonal">
                            <v-icon icon="mdi-clock-outline" size="14" class="mr-1" />
                            {{ item.scheduleFrequency }}
                        </v-chip>
                    </template>
                    <span v-else class="text-medium-emphasis">Desabilitado</span>
                </template>

                <!-- Last Backup -->
                <template #item.lastBackup="{ item }">
                    <template v-if="item.backups && item.backups.length > 0">
                        <v-chip :color="getStatusColor(item.backups[0]!.status)" size="small" variant="tonal">
                            {{ formatDate(item.backups[0]!.createdAt) }}
                        </v-chip>
                    </template>
                    <span v-else class="text-medium-emphasis">Nunca</span>
                </template>

                <!-- Actions -->
                <template #item.actions="{ item }">
                    <v-btn icon="mdi-play" size="small" variant="text" color="success" :loading="backupLoading[item.id]"
                        @click="runBackup(item)">
                        <v-icon icon="mdi-play" />
                        <v-tooltip activator="parent" location="top">
                            Executar Backup
                        </v-tooltip>
                    </v-btn>

                    <v-btn icon="mdi-connection" size="small" variant="text" color="info"
                        :loading="testLoading[item.id]" @click="testConnection(item)">
                        <v-icon icon="mdi-connection" />
                        <v-tooltip activator="parent" location="top">
                            Testar Conexão
                        </v-tooltip>
                    </v-btn>

                    <v-btn icon="mdi-pencil" size="small" variant="text" :to="`/connections/${item.id}/edit`">
                        <v-icon icon="mdi-pencil" />
                        <v-tooltip activator="parent" location="top">
                            Editar
                        </v-tooltip>
                    </v-btn>

                    <v-btn icon="mdi-delete" size="small" variant="text" color="error" @click="confirmDelete(item)">
                        <v-icon icon="mdi-delete" />
                        <v-tooltip activator="parent" location="top">
                            Excluir
                        </v-tooltip>
                    </v-btn>
                </template>

                <!-- No data -->
                <template #no-data>
                    <div class="text-center py-8">
                        <v-icon icon="mdi-database-off" size="64" color="grey" class="mb-4" />
                        <p class="text-h6 text-medium-emphasis mb-2">
                            Nenhuma conexão encontrada
                        </p>
                        <v-btn color="primary" variant="tonal" prepend-icon="mdi-plus" to="/connections/new">
                            Criar primeira conexão
                        </v-btn>
                    </div>
                </template>
            </v-data-table>
        </v-card>

        <!-- Delete Confirmation Dialog -->
        <v-dialog v-model="deleteDialog" max-width="400">
            <v-card>
                <v-card-title class="d-flex align-center">
                    <v-icon icon="mdi-alert" color="error" class="mr-2" />
                    Confirmar Exclusão
                </v-card-title>

                <v-card-text>
                    Tem certeza que deseja excluir a conexão
                    <strong>{{ connectionToDelete?.name }}</strong>?
                    <br /><br />
                    <v-alert type="warning" density="compact" variant="tonal">
                        Todos os backups associados também serão removidos!
                    </v-alert>
                </v-card-text>

                <v-card-actions>
                    <v-spacer />
                    <v-btn variant="text" @click="deleteDialog = false">
                        Cancelar
                    </v-btn>
                    <v-btn color="error" variant="flat" :loading="deleteLoading" @click="deleteConnection">
                        Excluir
                    </v-btn>
                </v-card-actions>
            </v-card>
        </v-dialog>
    </div>
</template>

<script lang="ts" setup>
import { ref, reactive, onMounted, inject } from 'vue'
import { connectionsApi } from '@/services/api'
import type { Connection, ConnectionStatus, DatabaseType } from '@/types/api'

const showNotification = inject<(msg: string, type: string) => void>('showNotification')

const loading = ref(false)
const connections = ref<Connection[]>([])
const testLoading = reactive<Record<number, boolean>>({})
const backupLoading = reactive<Record<number, boolean>>({})

const filters = reactive({
    search: '',
    type: null as DatabaseType | null,
    status: null as ConnectionStatus | null,
})

const headers = [
    { title: 'Nome', key: 'name', sortable: true },
    { title: 'Tipo', key: 'type', sortable: true },
    { title: 'Host', key: 'host', sortable: false },
    { title: 'Status', key: 'status', sortable: true },
    { title: 'Agendamento', key: 'schedule', sortable: false },
    { title: 'Último Backup', key: 'lastBackup', sortable: false },
    { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
]

const databaseTypes = [
    { title: 'MySQL', value: 'mysql' },
    { title: 'MariaDB', value: 'mariadb' },
    { title: 'PostgreSQL', value: 'postgresql' },
]

const statusOptions = [
    { title: 'Ativo', value: 'active' },
    { title: 'Inativo', value: 'inactive' },
    { title: 'Erro', value: 'error' },
]

async function loadConnections() {
    loading.value = true
    try {
        const response = await connectionsApi.list({
            search: filters.search || undefined,
            type: filters.type || undefined,
            status: filters.status || undefined,
        })
        connections.value = response.data?.data ?? []
    } catch (error) {
        console.error('Erro ao carregar conexões:', error)
        showNotification?.('Erro ao carregar conexões', 'error')
    } finally {
        loading.value = false
    }
}

let debounceTimer: ReturnType<typeof setTimeout>
function debouncedLoad() {
    clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
        loadConnections()
    }, 300)
}

async function testConnection(connection: Connection) {
    testLoading[connection.id] = true
    try {
        const response = await connectionsApi.test(connection.id)
        showNotification?.(
            `Conexão bem-sucedida! Latência: ${response.data?.latencyMs}ms`,
            'success'
        )
        loadConnections()
    } catch (error) {
        showNotification?.('Falha ao testar conexão', 'error')
    } finally {
        testLoading[connection.id] = false
    }
}

async function runBackup(connection: Connection) {
    backupLoading[connection.id] = true
    try {
        const response = await connectionsApi.backup(connection.id)
        showNotification?.(
            `Backup concluído: ${response.data?.fileName}`,
            'success'
        )
        loadConnections()
    } catch (error) {
        showNotification?.('Falha ao executar backup', 'error')
    } finally {
        backupLoading[connection.id] = false
    }
}

// Delete
const deleteDialog = ref(false)
const deleteLoading = ref(false)
const connectionToDelete = ref<Connection | null>(null)

function confirmDelete(connection: Connection) {
    connectionToDelete.value = connection
    deleteDialog.value = true
}

async function deleteConnection() {
    if (!connectionToDelete.value) return

    deleteLoading.value = true
    try {
        await connectionsApi.delete(connectionToDelete.value.id)
        showNotification?.('Conexão excluída com sucesso', 'success')
        deleteDialog.value = false
        loadConnections()
    } catch (error) {
        showNotification?.('Erro ao excluir conexão', 'error')
    } finally {
        deleteLoading.value = false
    }
}

// Helpers
function getDatabaseColor(type: DatabaseType): string {
    const colors: Record<DatabaseType, string> = {
        mysql: 'orange',
        mariadb: 'teal',
        postgresql: 'blue',
    }
    return colors[type] ?? 'grey'
}

function getDatabaseIcon(type: DatabaseType): string {
    return 'mdi-database'
}

function getStatusColor(status: ConnectionStatus | string): string {
    const colors: Record<string, string> = {
        active: 'success',
        inactive: 'grey',
        error: 'error',
        completed: 'success',
        failed: 'error',
        pending: 'warning',
        running: 'info',
    }
    return colors[status] ?? 'grey'
}

function getStatusLabel(status: ConnectionStatus): string {
    const labels: Record<ConnectionStatus, string> = {
        active: 'Ativo',
        inactive: 'Inativo',
        error: 'Erro',
    }
    return labels[status] ?? status
}

function formatDate(dateString: string): string {
    const date = new Date(dateString)
    return date.toLocaleString('pt-BR', {
        day: '2-digit',
        month: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
    })
}

onMounted(() => {
    loadConnections()
})
</script>
