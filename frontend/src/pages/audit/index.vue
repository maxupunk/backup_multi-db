<template>
  <div>
    <!-- Header -->
    <v-row align="center" class="mb-6">
      <v-col class="flex-grow-1" cols="12" sm="auto">
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">Logs de Auditoria</h1>
        <p class="text-body-2 text-medium-emphasis">
          Histórico de ações realizadas no sistema
        </p>
      </v-col>

      <v-col cols="12" sm="auto">
        <v-btn
          :block="!smAndUp"
          color="primary"
          :loading="loading"
          prepend-icon="mdi-refresh"
          @click="loadLogs"
        >
          Atualizar
        </v-btn>
      </v-col>
    </v-row>

    <!-- Stats Cards -->
    <v-row class="mb-6">
      <v-col cols="12" md="3" sm="6">
        <v-card class="h-100" color="primary" variant="tonal">
          <v-card-text class="d-flex align-center">
            <v-icon class="mr-4 opacity-70" icon="mdi-history" size="48" />
            <div>
              <div class="text-h4 font-weight-bold">{{ stats.total }}</div>
              <div class="text-caption">Total de Logs</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card class="h-100" color="info" variant="tonal">
          <v-card-text class="d-flex align-center">
            <v-icon class="mr-4 opacity-70" icon="mdi-calendar-today" size="48" />
            <div>
              <div class="text-h4 font-weight-bold">{{ stats.today }}</div>
              <div class="text-caption">Hoje</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card class="h-100" color="success" variant="tonal">
          <v-card-text class="d-flex align-center">
            <v-icon class="mr-4 opacity-70" icon="mdi-check-circle" size="48" />
            <div>
              <div class="text-h4 font-weight-bold">{{ stats.byStatus?.success ?? 0 }}</div>
              <div class="text-caption">Sucesso</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card class="h-100" color="error" variant="tonal">
          <v-card-text class="d-flex align-center">
            <v-icon class="mr-4 opacity-70" icon="mdi-alert-circle" size="48" />
            <div>
              <div class="text-h4 font-weight-bold">{{ stats.byStatus?.failure ?? 0 }}</div>
              <div class="text-caption">Falhas</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <!-- Filters -->
    <v-expansion-panels v-if="!mdAndUp" class="mb-6" variant="accordion">
      <v-expansion-panel>
        <v-expansion-panel-title>
          <div class="d-flex align-center">
            <v-icon class="mr-2" icon="mdi-filter-variant" />
            Filtros
          </div>
        </v-expansion-panel-title>
        <v-expansion-panel-text>
          <v-row>
            <v-col cols="12">
              <v-select
                v-model="filters.entityType"
                clearable
                density="comfortable"
                hide-details
                :items="entityTypeOptions"
                label="Entidade"
                variant="outlined"
                @update:model-value="loadLogs"
              />
            </v-col>

            <v-col cols="12">
              <v-select
                v-model="filters.action"
                clearable
                density="comfortable"
                hide-details
                :items="filteredActionOptions"
                label="Ação"
                variant="outlined"
                @update:model-value="loadLogs"
              />
            </v-col>

            <v-col cols="12">
              <v-select
                v-model="filters.status"
                clearable
                density="comfortable"
                hide-details
                :items="statusOptions"
                label="Status"
                variant="outlined"
                @update:model-value="loadLogs"
              />
            </v-col>

            <v-col cols="12">
              <v-text-field
                v-model="filters.entityId"
                clearable
                density="comfortable"
                hide-details
                label="ID Entidade"
                type="number"
                variant="outlined"
                @update:model-value="debouncedLoadLogs"
              />
            </v-col>
          </v-row>
        </v-expansion-panel-text>
      </v-expansion-panel>
    </v-expansion-panels>

    <v-card v-else class="mb-6">
      <v-card-text>
        <v-row>
          <v-col cols="12" sm="3">
            <v-select
              v-model="filters.entityType"
              clearable
              density="comfortable"
              hide-details
              :items="entityTypeOptions"
              label="Entidade"
              variant="outlined"
              @update:model-value="loadLogs"
            />
          </v-col>

          <v-col cols="12" sm="3">
            <v-select
              v-model="filters.action"
              clearable
              density="comfortable"
              hide-details
              :items="filteredActionOptions"
              label="Ação"
              variant="outlined"
              @update:model-value="loadLogs"
            />
          </v-col>

          <v-col cols="12" sm="3">
            <v-select
              v-model="filters.status"
              clearable
              density="comfortable"
              hide-details
              :items="statusOptions"
              label="Status"
              variant="outlined"
              @update:model-value="loadLogs"
            />
          </v-col>

          <v-col cols="12" sm="3">
            <v-text-field
              v-model="filters.entityId"
              clearable
              density="comfortable"
              hide-details
              label="ID Entidade"
              type="number"
              variant="outlined"
              @update:model-value="debouncedLoadLogs"
            />
          </v-col>
        </v-row>
      </v-card-text>
    </v-card>

    <!-- Logs Table -->
    <v-card>
      <v-data-table
        class="elevation-0"
        :headers="tableHeaders"
        :items="logs"
        :items-per-page="20"
        :loading="loading"
        :mobile="!mdAndUp"
      >
        <!-- Action -->
        <template #item.action="{ item }">
          <div class="d-flex align-center">
            <v-icon class="mr-2" :color="getActionColor(item.action)" :icon="item.actionIcon" size="20" />
            <span class="text-body-2">{{ item.actionDescription }}</span>
          </div>
          <div v-if="!mdAndUp" class="text-caption text-medium-emphasis mt-1">
            <span v-if="item.entityName">{{ item.entityName }}</span>
            <span v-if="item.entityId" class="ml-1">#{{ item.entityId }}</span>
          </div>
        </template>

        <!-- Entity -->
        <template #item.entity="{ item }">
          <div v-if="item.entityName" class="d-flex align-center">
            <v-chip class="mr-2" :color="getEntityColor(item.entityType)" label size="x-small">
              {{ getEntityLabel(item.entityType) }}
            </v-chip>
            <span class="font-weight-medium">{{ item.entityName }}</span>
            <span v-if="item.entityId" class="text-medium-emphasis ml-1">#{{ item.entityId }}</span>
          </div>
          <span v-else class="text-medium-emphasis">-</span>
        </template>

        <!-- Status -->
        <template #item.status="{ item }">
          <v-chip :color="item.statusColor" label size="small">
            <v-icon class="mr-1" :icon="getStatusIcon(item.status)" size="14" />
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
          <v-btn
            color="primary"
            icon="mdi-eye"
            size="small"
            variant="text"
            @click="showDetails(item)"
          >
            <v-icon icon="mdi-eye" />
            <v-tooltip activator="parent" location="top">
              Ver detalhes
            </v-tooltip>
          </v-btn>
        </template>

        <!-- No data -->
        <template #no-data>
          <div class="text-center py-8">
            <v-icon class="mb-4" color="grey" icon="mdi-history" size="64" />
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
        <v-pagination
          v-model="filters.page"
          :length="meta.lastPage"
          :total-visible="mdAndUp ? 5 : 3"
          @update:model-value="loadLogs"
        />
      </div>
    </v-card>

    <!-- Details Dialog -->
    <v-dialog v-model="detailsDialog" max-width="600">
      <v-card v-if="selectedLog">
        <v-card-title class="d-flex align-center py-4">
          <v-icon class="mr-2" :color="getActionColor(selectedLog.action)" :icon="selectedLog.actionIcon" />
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
              <v-chip class="mt-1" :color="selectedLog.statusColor" label size="small">
                {{ getStatusLabel(selectedLog.status) }}
              </v-chip>
            </v-col>

            <v-col cols="12" sm="6">
              <div class="text-caption text-medium-emphasis">Entidade</div>
              <div class="d-flex align-center">
                <v-chip
                  class="mr-2"
                  :color="getEntityColor(selectedLog.entityType)"
                  label
                  size="x-small"
                >
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
              <v-alert density="compact" type="error" variant="tonal">
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
  import type { AuditAction, AuditEntityType, AuditLog, AuditStats, AuditStatus } from '@/types/api'
  import { computed, onMounted, reactive, ref } from 'vue'
  import { useDisplay } from 'vuetify'
  import { auditLogsApi } from '@/services/api'

  const { smAndUp, mdAndUp } = useDisplay()

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

  const desktopHeaders = [
    { title: 'Ação', key: 'action', sortable: false },
    { title: 'Entidade', key: 'entity', sortable: false },
    { title: 'Status', key: 'status', sortable: false },
    { title: 'IP', key: 'ipAddress', sortable: false },
    { title: 'Data', key: 'createdAt', sortable: true },
    { title: '', key: 'actions', sortable: false, align: 'end' as const, width: '80px' },
  ]

  const mobileHeaders = [
    { title: 'Ação', key: 'action', sortable: false },
    { title: 'Status', key: 'status', sortable: false },
    { title: 'Data', key: 'createdAt', sortable: true },
    { title: '', key: 'actions', sortable: false, align: 'end' as const, width: '56px' },
  ]

  const tableHeaders = computed(() => (mdAndUp.value ? desktopHeaders : mobileHeaders))

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

  async function loadLogs () {
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

  async function loadStats () {
    try {
      const response = await auditLogsApi.stats()
      stats.value = response.data ?? {}
    } catch (error) {
      console.error('Erro ao carregar estatísticas:', error)
    }
  }

  function showDetails (log: AuditLog) {
    selectedLog.value = log
    detailsDialog.value = true
  }

  // Debounce for entityId input
  let debounceTimer: ReturnType<typeof setTimeout>
  function debouncedLoadLogs () {
    clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
      loadLogs()
    }, 500)
  }

  // Helpers
  function getActionColor (action: AuditAction): string {
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

  function getEntityColor (entityType: AuditEntityType): string {
    const colors: Record<AuditEntityType, string> = {
      connection: 'purple',
      backup: 'blue',
      settings: 'orange',
    }
    return colors[entityType] ?? 'grey'
  }

  function getEntityLabel (entityType: AuditEntityType): string {
    const labels: Record<AuditEntityType, string> = {
      connection: 'Conexão',
      backup: 'Backup',
      settings: 'Config',
    }
    return labels[entityType] ?? entityType
  }

  function getStatusIcon (status: AuditStatus): string {
    const icons: Record<AuditStatus, string> = {
      success: 'mdi-check',
      failure: 'mdi-alert-circle',
      warning: 'mdi-alert',
    }
    return icons[status] ?? 'mdi-help'
  }

  function getStatusLabel (status: AuditStatus): string {
    const labels: Record<AuditStatus, string> = {
      success: 'Sucesso',
      failure: 'Falha',
      warning: 'Aviso',
    }
    return labels[status] ?? status
  }

  function formatDate (dateString: string): string {
    const date = new Date(dateString)
    return date.toLocaleString('pt-BR', {
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
  }

  function formatTimeAgo (dateString: string): string {
    const date = new Date(dateString)
    const now = new Date()
    const diff = now.getTime() - date.getTime()

    const minutes = Math.floor(diff / 60_000)
    const hours = Math.floor(diff / 3_600_000)
    const days = Math.floor(diff / 86_400_000)

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
