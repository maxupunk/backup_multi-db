<template>
    <div>
        <!-- Header -->
        <div class="d-flex align-center justify-space-between mb-6">
            <div>
                <h1 class="text-h4 font-weight-bold mb-1">Backups</h1>
                <p class="text-body-2 text-medium-emphasis">
                    Histórico de backups realizados
                </p>
            </div>

            <v-btn color="primary" prepend-icon="mdi-refresh" :loading="loading" @click="loadBackups">
                Atualizar
            </v-btn>
        </div>

        <!-- Filters -->
        <v-card class="mb-6">
            <v-card-text>
                <v-row>
                    <v-col cols="12" sm="4">
                        <v-select v-model="filters.connectionId" label="Conexão" :items="connections" item-title="name"
                            item-value="id" clearable hide-details @update:model-value="loadBackups" />
                    </v-col>

                    <v-col cols="12" sm="4">
                        <v-select v-model="filters.status" label="Status" :items="statusOptions" clearable hide-details
                            @update:model-value="loadBackups" />
                    </v-col>

                    <v-col cols="12" sm="4">
                        <v-text-field v-model="filters.search" label="Buscar" prepend-inner-icon="mdi-magnify" clearable
                            hide-details disabled hint="Em breve" />
                    </v-col>
                </v-row>
            </v-card-text>
        </v-card>

        <!-- Backups List -->
        <v-card>
            <v-data-table :headers="headers" :items="backups" :loading="loading" :items-per-page="20"
                class="elevation-0">
                <!-- Connection -->
                <template #item.connection="{ item }">
                    <div v-if="item.connection" class="d-flex align-center">
                        <v-chip :color="getDatabaseColor(item.connection.type)" size="x-small" label class="mr-2">
                            {{ item.connection.type.toUpperCase() }}
                        </v-chip>
                        <span class="font-weight-medium">{{ item.connection.name }}</span>
                    </div>
                    <span v-else class="text-medium-emphasis">N/A</span>
                </template>

                <!-- Status -->
                <template #item.status="{ item }">
                    <v-chip :color="getStatusColor(item.status)" size="small" label>
                        <v-icon :icon="getStatusIcon(item.status)" size="14" class="mr-1" />
                        {{ getStatusLabel(item.status) }}
                    </v-chip>
                </template>

                <!-- File Size -->
                <template #item.fileSize="{ item }">
                    <span v-if="item.fileSize" class="font-weight-medium">
                        {{ formatFileSize(item.fileSize) }}
                    </span>
                    <span v-else class="text-medium-emphasis">-</span>
                </template>

                <!-- Duration -->
                <template #item.duration="{ item }">
                    <span v-if="item.durationSeconds">
                        {{ formatDuration(item.durationSeconds) }}
                    </span>
                    <span v-else class="text-medium-emphasis">-</span>
                </template>

                <!-- Retention -->
                <template #item.retentionType="{ item }">
                    <v-chip :color="getRetentionColor(item.retentionType)" size="small" variant="tonal">
                        {{ getRetentionLabel(item.retentionType) }}
                    </v-chip>
                </template>

                <!-- Trigger -->
                <template #item.trigger="{ item }">
                    <v-icon :icon="item.trigger === 'scheduled' ? 'mdi-clock-outline' : 'mdi-hand-pointing-right'"
                        :color="item.trigger === 'scheduled' ? 'info' : 'warning'" size="20" />
                    <v-tooltip activator="parent" location="top">
                        {{ item.trigger === 'scheduled' ? 'Agendado' : 'Manual' }}
                    </v-tooltip>
                </template>

                <!-- Created At -->
                <template #item.createdAt="{ item }">
                    {{ formatDate(item.createdAt) }}
                </template>

                <!-- Actions -->
                <template #item.actions="{ item }">
                    <v-btn v-if="item.status === 'completed' && item.filePath" icon="mdi-download" size="small"
                        variant="text" color="primary" :href="getDownloadUrl(item.id)" target="_blank">
                        <v-icon icon="mdi-download" />
                        <v-tooltip activator="parent" location="top">
                            Download
                        </v-tooltip>
                    </v-btn>

                    <v-btn icon="mdi-delete" size="small" variant="text" color="error" :disabled="item.protected"
                        @click="confirmDelete(item)">
                        <v-icon icon="mdi-delete" />
                        <v-tooltip activator="parent" location="top">
                            {{ item.protected ? 'Protegido' : 'Excluir' }}
                        </v-tooltip>
                    </v-btn>
                </template>

                <!-- No data -->
                <template #no-data>
                    <div class="text-center py-8">
                        <v-icon icon="mdi-backup-restore" size="64" color="grey" class="mb-4" />
                        <p class="text-h6 text-medium-emphasis mb-2">
                            Nenhum backup encontrado
                        </p>
                        <v-btn color="primary" variant="tonal" prepend-icon="mdi-database" to="/connections">
                            Ir para Conexões
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
                    Tem certeza que deseja excluir este backup?
                    <br /><br />
                    <strong>{{ backupToDelete?.fileName }}</strong>
                </v-card-text>

                <v-card-actions>
                    <v-spacer />
                    <v-btn variant="text" @click="deleteDialog = false">
                        Cancelar
                    </v-btn>
                    <v-btn color="error" variant="flat" :loading="deleteLoading" @click="deleteBackup">
                        Excluir
                    </v-btn>
                </v-card-actions>
            </v-card>
        </v-dialog>
    </div>
</template>

<script lang="ts" setup>
import { ref, reactive, onMounted, inject } from 'vue'
import { backupsApi, connectionsApi } from '@/services/api'
import type { Backup, BackupStatus, Connection, DatabaseType, RetentionType } from '@/types/api'

const showNotification = inject<(msg: string, type: string) => void>('showNotification')

const loading = ref(false)
const backups = ref<Backup[]>([])
const connections = ref<Connection[]>([])

const filters = reactive({
    connectionId: null as number | null,
    status: null as BackupStatus | null,
    search: '',
})

const headers = [
    { title: 'Conexão', key: 'connection', sortable: false },
    { title: 'Status', key: 'status', sortable: true },
    { title: 'Tamanho', key: 'fileSize', sortable: true },
    { title: 'Duração', key: 'duration', sortable: false },
    { title: 'Retenção', key: 'retentionType', sortable: true },
    { title: 'Tipo', key: 'trigger', sortable: false, align: 'center' as const },
    { title: 'Data', key: 'createdAt', sortable: true },
    { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
]

const statusOptions = [
    { title: 'Pendente', value: 'pending' },
    { title: 'Em execução', value: 'running' },
    { title: 'Concluído', value: 'completed' },
    { title: 'Falhou', value: 'failed' },
    { title: 'Cancelado', value: 'cancelled' },
]

async function loadBackups() {
    loading.value = true
    try {
        const response = await backupsApi.list({
            connectionId: filters.connectionId || undefined,
            status: filters.status || undefined,
        })
        backups.value = response.data?.data ?? []
    } catch (error) {
        console.error('Erro ao carregar backups:', error)
        showNotification?.('Erro ao carregar backups', 'error')
    } finally {
        loading.value = false
    }
}

async function loadConnections() {
    try {
        const response = await connectionsApi.list()
        connections.value = response.data?.data ?? []
    } catch (error) {
        console.error('Erro ao carregar conexões:', error)
    }
}

// Delete
const deleteDialog = ref(false)
const deleteLoading = ref(false)
const backupToDelete = ref<Backup | null>(null)

function confirmDelete(backup: Backup) {
    backupToDelete.value = backup
    deleteDialog.value = true
}

async function deleteBackup() {
    if (!backupToDelete.value) return

    deleteLoading.value = true
    try {
        await backupsApi.delete(backupToDelete.value.id)
        showNotification?.('Backup excluído com sucesso', 'success')
        deleteDialog.value = false
        loadBackups()
    } catch (error) {
        showNotification?.('Erro ao excluir backup', 'error')
    } finally {
        deleteLoading.value = false
    }
}

// Helpers
function getDownloadUrl(id: number): string {
    return backupsApi.getDownloadUrl(id)
}

function getDatabaseColor(type: DatabaseType): string {
    const colors: Record<DatabaseType, string> = {
        mysql: 'orange',
        mariadb: 'teal',
        postgresql: 'blue',
    }
    return colors[type] ?? 'grey'
}

function getStatusColor(status: BackupStatus): string {
    const colors: Record<BackupStatus, string> = {
        pending: 'warning',
        running: 'info',
        completed: 'success',
        failed: 'error',
        cancelled: 'grey',
    }
    return colors[status] ?? 'grey'
}

function getStatusIcon(status: BackupStatus): string {
    const icons: Record<BackupStatus, string> = {
        pending: 'mdi-clock-outline',
        running: 'mdi-loading mdi-spin',
        completed: 'mdi-check',
        failed: 'mdi-alert-circle',
        cancelled: 'mdi-cancel',
    }
    return icons[status] ?? 'mdi-help'
}

function getStatusLabel(status: BackupStatus): string {
    const labels: Record<BackupStatus, string> = {
        pending: 'Pendente',
        running: 'Em execução',
        completed: 'Concluído',
        failed: 'Falhou',
        cancelled: 'Cancelado',
    }
    return labels[status] ?? status
}

function getRetentionColor(type: RetentionType): string {
    const colors: Record<RetentionType, string> = {
        hourly: 'grey',
        daily: 'blue',
        weekly: 'purple',
        monthly: 'orange',
        yearly: 'red',
    }
    return colors[type] ?? 'grey'
}

function getRetentionLabel(type: RetentionType): string {
    const labels: Record<RetentionType, string> = {
        hourly: 'Horário',
        daily: 'Diário',
        weekly: 'Semanal',
        monthly: 'Mensal',
        yearly: 'Anual',
    }
    return labels[type] ?? type
}

function formatFileSize(bytes: number): string {
    const units = ['B', 'KB', 'MB', 'GB', 'TB']
    let size = bytes
    let unitIndex = 0

    while (size >= 1024 && unitIndex < units.length - 1) {
        size /= 1024
        unitIndex++
    }

    return `${size.toFixed(1)} ${units[unitIndex]}`
}

function formatDuration(seconds: number): string {
    if (seconds < 60) return `${seconds}s`
    const minutes = Math.floor(seconds / 60)
    const secs = seconds % 60
    return `${minutes}m ${secs}s`
}

function formatDate(dateString: string): string {
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
    loadConnections()
    loadBackups()
})
</script>
