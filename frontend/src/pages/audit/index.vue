<template>
    <div>
        <!-- Header -->
        <div class="d-flex align-center justify-space-between mb-6">
            <div>
                <h1 class="text-h4 font-weight-bold mb-1">Logs de Auditoria</h1>
                <p class="text-body-2 text-medium-emphasis">
                    Histórico de ações realizadas no sistema
                </p>
            </div>

            <v-btn color="primary" prepend-icon="mdi-refresh" :loading="loading" @click="loadLogs">
                Atualizar
            </v-btn>
        </div>

        <!-- Stats Cards -->
        <v-row class="mb-6">
            <v-col cols="12" sm="6" md="3">
                <v-card class="h-100" variant="tonal" color="primary">
                    <v-card-text class="d-flex align-center">
                        <v-icon icon="mdi-history" size="48" class="mr-4 opacity-70" />
                        <div>
                            <div class="text-h4 font-weight-bold">{{ stats.total }}</div>
                            <div class="text-caption">Total de Logs</div>
                        </div>
                    </v-card-text>
                </v-card>
            </v-col>

            <v-col cols="12" sm="6" md="3">
                <v-card class="h-100" variant="tonal" color="info">
                    <v-card-text class="d-flex align-center">
                        <v-icon icon="mdi-calendar-today" size="48" class="mr-4 opacity-70" />
                        <div>
                            <div class="text-h4 font-weight-bold">{{ stats.today }}</div>
                            <div class="text-caption">Hoje</div>
                        </div>
                    </v-card-text>
                </v-card>
            </v-col>

            <v-col cols="12" sm="6" md="3">
                <v-card class="h-100" variant="tonal" color="success">
                    <v-card-text class="d-flex align-center">
                        <v-icon icon="mdi-check-circle" size="48" class="mr-4 opacity-70" />
                        <div>
                            <div class="text-h4 font-weight-bold">{{ stats.byStatus?.success ?? 0 }}</div>
                            <div class="text-caption">Sucesso</div>
                        </div>
                    </v-card-text>
                </v-card>
            </v-col>

            <v-col cols="12" sm="6" md="3">
                <v-card class="h-100" variant="tonal" color="error">
                    <v-card-text class="d-flex align-center">
                        <v-icon icon="mdi-alert-circle" size="48" class="mr-4 opacity-70" />
                        <div>
                            <div class="text-h4 font-weight-bold">{{ stats.byStatus?.failure ?? 0 }}</div>
                            <div class="text-caption">Falhas</div>
                        </div>
                    </v-card-text>
                </v-card>
            </v-col>
        </v-row>

        <!-- Filters -->
        <v-card class="mb-6">
            <v-card-text>
                <v-row>
                    <v-col cols="12" sm="3">
                        <v-select v-model="filters.entityType" label="Entidade" :items="entityTypeOptions" clearable
                            hide-details @update:model-value="loadLogs" />
                    </v-col>

                    <v-col cols="12" sm="3">
                        <v-select v-model="filters.action" label="Ação" :items="filteredActionOptions" clearable
                            hide-details @update:model-value="loadLogs" />
                    </v-col>

                    <v-col cols="12" sm="3">
                        <v-select v-model="filters.status" label="Status" :items="statusOptions" clearable hide-details
                            @update:model-value="loadLogs" />
                    </v-col>

                    <v-col cols="12" sm="3">
                        <v-text-field v-model="filters.entityId" label="ID Entidade" type="number" clearable
                            hide-details @update:model-value="debouncedLoadLogs" />
                    </v-col>
                </v-row>
            </v-card-text>
        </v-card>

        <!-- Logs Table -->
        <v-card>
            <v-data-table :headers="headers" :items="logs" :loading="loading" :items-per-page="20" class="elevation-0">
                <!-- Action -->
                <template #item.action="{ item }">
                    <div class="d-flex align-center">
                        <v-icon :icon="item.actionIcon" :color="getActionColor(item.action)" size="20" class="mr-2" />
                        <span class="text-body-2">{{ item.actionDescription }}</span>
                    </div>
                </template>

                <!-- Entity -->
                <template #item.entity="{ item }">
                    <div v-if="item.entityName" class="d-flex align-center">
                        <v-chip :color="getEntityColor(item.entityType)" size="x-small" label class="mr-2">
                            {{ getEntityLabel(item.entityType) }}
                        </v-chip>
                        <span class="font-weight-medium">{{ item.entityName }}</span>
                        <span v-if="item.entityId" class="text-medium-emphasis ml-1">#{{ item.entityId }}</span>
                    </div>
                    <span v-else class="text-medium-emphasis">-</span>
                </template>

                <!-- Status -->
                <template #item.status="{ item }">
                    <v-chip :color="item.statusColor" size="small" label>
                        <v-icon :icon="getStatusIcon(item.status)" size="14" class="mr-1" />
                        {{ getStatusLabel(item.status) }}
                    </v-chip>
                </template>

                <!-- IP Address -->
                <template #item.ipAddress="{ item }">
                    <v-chip v-if="item.ipAddress" size="x-small" variant="outlined">
                        {{ item.ipAddress }}
                    </v-chip>
                    <span v-else class="text-medium-emphasis">-</span>
                </template>

                <!-- Created At -->
                <template #item.createdAt="{ item }">
                    <div>
                        <div class="font-weight-medium">{{ formatDate(item.createdAt) }}</div>
                        <div class="text-caption text-medium-emphasis">{{ formatTimeAgo(item.createdAt) }}</div>
                    </div>
                </template>

                <!-- Actions -->
                <template #item.actions="{ item }">
                    <v-btn icon="mdi-eye" size="small" variant="text" color="primary" @click="showDetails(item)">
                        <v-icon icon="mdi-eye" />
                        <v-tooltip activator="parent" location="top">
                            Ver detalhes
                        </v-tooltip>
                    </v-btn>
                </template>

                <!-- No data -->
                <template #no-data>
                    <div class="text-center py-8">
                        <v-icon icon="mdi-history" size="64" color="grey" class="mb-4" />
                        <p class="text-h6 text-medium-emphasis mb-2">
                            Nenhum log encontrado
                        </p>
                        <p class="text-body-2 text-medium-emphasis">
                            Os logs de auditoria aparecerão aqui conforme as ações são realizadas
                        </p>
                    </div>
                </template>
            </v-data-table>

            <!-- Pagination -->
            <div v-if="meta.lastPage > 1" class="d-flex justify-center pa-4">
                <v-pagination v-model="filters.page" :length="meta.lastPage" :total-visible="5"
                    @update:model-value="loadLogs" />
            </div>
        </v-card>

        <!-- Details Dialog -->
        <v-dialog v-model="detailsDialog" max-width="600">
            <v-card v-if="selectedLog">
                <v-card-title class="d-flex align-center py-4">
                    <v-icon :icon="selectedLog.actionIcon" :color="getActionColor(selectedLog.action)" class="mr-2" />
                    {{ selectedLog.actionDescription }}
                </v-card-title>

                <v-divider />

                <v-card-text class="py-4">
                    <v-row dense>
                        <v-col cols="12" sm="6">
                            <div class="text-caption text-medium-emphasis">Descrição</div>
                            <div class="font-weight-medium">{{ selectedLog.description }}</div>
                        </v-col>

                        <v-col cols="12" sm="6">
                            <div class="text-caption text-medium-emphasis">Status</div>
                            <v-chip :color="selectedLog.statusColor" size="small" label class="mt-1">
                                {{ getStatusLabel(selectedLog.status) }}
                            </v-chip>
                        </v-col>

                        <v-col cols="12" sm="6">
                            <div class="text-caption text-medium-emphasis">Entidade</div>
                            <div class="d-flex align-center">
                                <v-chip :color="getEntityColor(selectedLog.entityType)" size="x-small" label
                                    class="mr-2">
                                    {{ getEntityLabel(selectedLog.entityType) }}
                                </v-chip>
                                <span v-if="selectedLog.entityName">{{ selectedLog.entityName }}</span>
                                <span v-if="selectedLog.entityId" class="text-medium-emphasis ml-1">
                                    #{{ selectedLog.entityId }}
                                </span>
                            </div>
                        </v-col>

                        <v-col cols="12" sm="6">
                            <div class="text-caption text-medium-emphasis">Data/Hora</div>
                            <div class="font-weight-medium">{{ formatDate(selectedLog.createdAt) }}</div>
                        </v-col>

                        <v-col cols="12" sm="6">
                            <div class="text-caption text-medium-emphasis">IP</div>
                            <div>{{ selectedLog.ipAddress ?? '-' }}</div>
                        </v-col>

                        <v-col cols="12" sm="6">
                            <div class="text-caption text-medium-emphasis">User Agent</div>
                            <div class="text-truncate" style="max-width: 250px;">
                                {{ selectedLog.userAgent ?? '-' }}
                            </div>
                        </v-col>

                        <v-col v-if="selectedLog.errorMessage" cols="12">
                            <v-alert type="error" variant="tonal" density="compact">
                                {{ selectedLog.errorMessage }}
                            </v-alert>
                        </v-col>

                        <v-col v-if="selectedLog.details && Object.keys(selectedLog.details).length > 0" cols="12">
                            <div class="text-caption text-medium-emphasis mb-2">Detalhes</div>
                            <v-card variant="outlined">
                                <v-card-text>
                                    <pre class="text-caption" style="white-space: pre-wrap; word-break: break-word;">{{
                                        JSON.stringify(selectedLog.details, null, 2) }}</pre>
                                </v-card-text>
                            </v-card>
                        </v-col>
                    </v-row>
                </v-card-text>

                <v-divider />

                <v-card-actions>
                    <v-spacer />
                    <v-btn variant="text" @click="detailsDialog = false">
                        Fechar
                    </v-btn>
                </v-card-actions>
            </v-card>
        </v-dialog>
    </div>
</template>

<script lang="ts" setup>
import { ref, reactive, computed, onMounted } from 'vue'
import { auditLogsApi } from '@/services/api'
import type { AuditLog, AuditAction, AuditEntityType, AuditStatus, AuditStats } from '@/types/api'

const loading = ref(false)
const logs = ref<AuditLog[]>([])
const stats = ref<Partial<AuditStats>>({})
const meta = ref({ total: 0, perPage: 20, currentPage: 1, lastPage: 1 })

const filters = reactive({
    page: 1,
    entityType: null as AuditEntityType | null,
    action: null as AuditAction | null,
    status: null as AuditStatus | null,
    entityId: null as number | null,
})

const headers = [
    { title: 'Ação', key: 'action', sortable: false },
    { title: 'Entidade', key: 'entity', sortable: false },
    { title: 'Status', key: 'status', sortable: false },
    { title: 'IP', key: 'ipAddress', sortable: false },
    { title: 'Data', key: 'createdAt', sortable: true },
    { title: '', key: 'actions', sortable: false, align: 'end' as const, width: '80px' },
]

const entityTypeOptions = [
    { title: 'Conexão', value: 'connection' },
    { title: 'Backup', value: 'backup' },
    { title: 'Configurações', value: 'settings' },
]

const allActionOptions = [
    { title: 'Conexão Criada', value: 'connection.created', entity: 'connection' },
    { title: 'Conexão Atualizada', value: 'connection.updated', entity: 'connection' },
    { title: 'Conexão Removida', value: 'connection.deleted', entity: 'connection' },
    { title: 'Conexão Testada', value: 'connection.tested', entity: 'connection' },
    { title: 'Backup Iniciado', value: 'backup.started', entity: 'backup' },
    { title: 'Backup Concluído', value: 'backup.completed', entity: 'backup' },
    { title: 'Backup Falhou', value: 'backup.failed', entity: 'backup' },
    { title: 'Backup Removido', value: 'backup.deleted', entity: 'backup' },
    { title: 'Backup Baixado', value: 'backup.downloaded', entity: 'backup' },
    { title: 'Configurações Atualizadas', value: 'settings.updated', entity: 'settings' },
]

const filteredActionOptions = computed(() => {
    if (!filters.entityType) return allActionOptions
    return allActionOptions.filter(opt => opt.entity === filters.entityType)
})

const statusOptions = [
    { title: 'Sucesso', value: 'success' },
    { title: 'Falha', value: 'failure' },
    { title: 'Aviso', value: 'warning' },
]

// Details dialog
const detailsDialog = ref(false)
const selectedLog = ref<AuditLog | null>(null)

async function loadLogs() {
    loading.value = true
    try {
        const response = await auditLogsApi.list({
            page: filters.page,
            limit: 20,
            action: filters.action || undefined,
            entityType: filters.entityType || undefined,
            entityId: filters.entityId || undefined,
            status: filters.status || undefined,
        })
        logs.value = response.data ?? []
        meta.value = response.meta ?? { total: 0, perPage: 20, currentPage: 1, lastPage: 1 }
    } catch (error) {
        console.error('Erro ao carregar logs:', error)
    } finally {
        loading.value = false
    }
}

async function loadStats() {
    try {
        const response = await auditLogsApi.stats()
        stats.value = response.data ?? {}
    } catch (error) {
        console.error('Erro ao carregar estatísticas:', error)
    }
}

function showDetails(log: AuditLog) {
    selectedLog.value = log
    detailsDialog.value = true
}

// Debounce for entityId input
let debounceTimer: ReturnType<typeof setTimeout>
function debouncedLoadLogs() {
    clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
        loadLogs()
    }, 500)
}

// Helpers
function getActionColor(action: AuditAction): string {
    if (action.includes('created')) return 'success'
    if (action.includes('updated')) return 'info'
    if (action.includes('deleted')) return 'error'
    if (action.includes('tested')) return 'warning'
    if (action.includes('started')) return 'info'
    if (action.includes('completed')) return 'success'
    if (action.includes('failed')) return 'error'
    if (action.includes('downloaded')) return 'primary'
    return 'grey'
}

function getEntityColor(entityType: AuditEntityType): string {
    const colors: Record<AuditEntityType, string> = {
        connection: 'purple',
        backup: 'blue',
        settings: 'orange',
    }
    return colors[entityType] ?? 'grey'
}

function getEntityLabel(entityType: AuditEntityType): string {
    const labels: Record<AuditEntityType, string> = {
        connection: 'Conexão',
        backup: 'Backup',
        settings: 'Config',
    }
    return labels[entityType] ?? entityType
}

function getStatusIcon(status: AuditStatus): string {
    const icons: Record<AuditStatus, string> = {
        success: 'mdi-check',
        failure: 'mdi-alert-circle',
        warning: 'mdi-alert',
    }
    return icons[status] ?? 'mdi-help'
}

function getStatusLabel(status: AuditStatus): string {
    const labels: Record<AuditStatus, string> = {
        success: 'Sucesso',
        failure: 'Falha',
        warning: 'Aviso',
    }
    return labels[status] ?? status
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

function formatTimeAgo(dateString: string): string {
    const date = new Date(dateString)
    const now = new Date()
    const diff = now.getTime() - date.getTime()

    const minutes = Math.floor(diff / 60000)
    const hours = Math.floor(diff / 3600000)
    const days = Math.floor(diff / 86400000)

    if (minutes < 1) return 'agora'
    if (minutes < 60) return `há ${minutes}min`
    if (hours < 24) return `há ${hours}h`
    if (days < 7) return `há ${days}d`
    return ''
}

onMounted(() => {
    loadStats()
    loadLogs()
})
</script>
