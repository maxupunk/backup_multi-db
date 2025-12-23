<template>
  <div>
    <!-- Header -->
    <v-row align="center" class="mb-6">
      <v-col class="flex-grow-1" cols="12" sm="auto">
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">Backups</h1>
        <p class="text-body-2 text-medium-emphasis">
          Histórico de backups realizados
        </p>
      </v-col>

      <v-col cols="12" sm="auto">
        <v-btn :block="!smAndUp" color="primary" :loading="loading" prepend-icon="mdi-refresh" @click="loadBackups">
          Atualizar
        </v-btn>
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
              <v-select v-model="filters.connectionId" clearable density="comfortable" hide-details :items="connections"
                item-title="name" item-value="id" label="Conexão" variant="outlined"
                @update:model-value="loadBackups" />
            </v-col>

            <v-col cols="12">
              <v-select v-model="filters.status" clearable density="comfortable" hide-details :items="statusOptions"
                label="Status" variant="outlined" @update:model-value="loadBackups" />
            </v-col>

            <v-col cols="12">
              <v-text-field v-model="filters.search" clearable density="comfortable" disabled hide-details
                hint="Em breve" label="Buscar" prepend-inner-icon="mdi-magnify" variant="outlined" />
            </v-col>
          </v-row>
        </v-expansion-panel-text>
      </v-expansion-panel>
    </v-expansion-panels>

    <v-card v-else class="mb-6">
      <v-card-text>
        <v-row>
          <v-col cols="12" sm="4">
            <v-select v-model="filters.connectionId" clearable density="comfortable" hide-details :items="connections"
              item-title="name" item-value="id" label="Conexão" variant="outlined" @update:model-value="loadBackups" />
          </v-col>

          <v-col cols="12" sm="4">
            <v-select v-model="filters.status" clearable density="comfortable" hide-details :items="statusOptions"
              label="Status" variant="outlined" @update:model-value="loadBackups" />
          </v-col>

          <v-col cols="12" sm="4">
            <v-text-field v-model="filters.search" clearable density="comfortable" disabled hide-details hint="Em breve"
              label="Buscar" prepend-inner-icon="mdi-magnify" variant="outlined" />
          </v-col>
        </v-row>
      </v-card-text>
    </v-card>

    <!-- Backups List -->
    <v-card>
      <v-data-table class="elevation-0" :headers="tableHeaders" :items="backups" :items-per-page="mdAndUp ? 20 : 10"
        :loading="loading" :mobile="!mdAndUp">
        <!-- Connection -->
        <template #item.connection="{ item }">
          <div v-if="item.connection" class="d-flex align-center">
            <v-chip class="mr-2" :color="getDatabaseColor(item.connection.type)" label size="x-small">
              {{ item.connection.type.toUpperCase() }}
            </v-chip>
            <span class="font-weight-medium">{{ item.connection.name }}</span>
          </div>
          <span v-else class="text-medium-emphasis">N/A</span>

          <div v-if="!mdAndUp" class="text-caption text-medium-emphasis mt-1">
            <span v-if="item.fileSize">{{ formatFileSize(item.fileSize) }}</span>
            <span v-if="item.durationSeconds"> • {{ formatDuration(item.durationSeconds) }}</span>
            <span v-if="item.retentionType"> • {{ getRetentionLabel(item.retentionType) }}</span>
            <span v-if="item.trigger"> • {{ item.trigger === 'scheduled' ? 'Agendado' : 'Manual' }}</span>
          </div>
        </template>

        <!-- Status -->
        <template #item.status="{ item }">
          <v-chip :color="getStatusColor(item.status)" label size="small">
            <v-icon class="mr-1" :icon="getStatusIcon(item.status)" size="14" />
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
          <v-icon :color="item.trigger === 'scheduled' ? 'info' : 'warning'"
            :icon="item.trigger === 'scheduled' ? 'mdi-clock-outline' : 'mdi-hand-pointing-right'" size="20" />
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
          <template v-if="mdAndUp">
            <v-btn v-if="item.status === 'completed' && item.filePath" color="primary" icon="mdi-download"
              :loading="downloadingId === item.id" size="small" variant="text" @click="downloadBackup(item)">
              <v-icon icon="mdi-download" />
              <v-tooltip activator="parent" location="top">
                Download
              </v-tooltip>
            </v-btn>

            <v-btn color="error" :disabled="item.protected" icon="mdi-delete" size="small" variant="text"
              @click="confirmDelete(item)">
              <v-icon icon="mdi-delete" />
              <v-tooltip activator="parent" location="top">
                {{ item.protected ? 'Protegido' : 'Excluir' }}
              </v-tooltip>
            </v-btn>
          </template>

          <v-menu v-else location="bottom end">
            <template #activator="{ props }">
              <v-btn v-bind="props" icon="mdi-dots-vertical" variant="text" />
            </template>
            <v-list density="compact">
              <v-list-item v-if="item.status === 'completed' && item.filePath" :disabled="downloadingId === item.id"
                prepend-icon="mdi-download" title="Download" @click="downloadBackup(item)" />
              <v-list-item :disabled="item.protected" prepend-icon="mdi-delete" title="Excluir"
                @click="confirmDelete(item)" />
            </v-list>
          </v-menu>
        </template>

        <!-- No data -->
        <template #no-data>
          <div class="text-center py-8">
            <v-icon class="mb-4" color="grey" icon="mdi-backup-restore" size="64" />
            <p class="text-h6 text-medium-emphasis mb-2">
              Nenhum backup encontrado
            </p>
            <v-btn color="primary" prepend-icon="mdi-database" to="/connections" variant="tonal">
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
          <v-icon class="mr-2" color="error" icon="mdi-alert" />
          Confirmar Exclusão
        </v-card-title>

        <v-card-text>
          Tem certeza que deseja excluir este backup?
          <br><br>
          <strong>{{ backupToDelete?.fileName }}</strong>
        </v-card-text>

        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="deleteDialog = false">
            Cancelar
          </v-btn>
          <v-btn color="error" :loading="deleteLoading" variant="flat" @click="deleteBackup">
            Excluir
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script lang="ts" setup>
import type { Backup, BackupStatus, Connection, DatabaseType, RetentionType } from '@/types/api'
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue'
import { useDisplay } from 'vuetify'
import { backupsApi, connectionsApi } from '@/services/api'
import { useNotifier } from '@/composables/useNotifier'
import { useNotificationStore } from '@/stores/notification'
import {
  backupStatusOptions,
  getBackupStatusColor as getStatusColor,
  getBackupStatusIcon as getStatusIcon,
  getBackupStatusLabel as getStatusLabel,
} from '@/ui/backup'
import { getDatabaseColor } from '@/ui/database'
import { formatDateTimePtBR as formatDate, formatDuration, formatFileSize } from '@/utils/format'

const notify = useNotifier()
const notificationStore = useNotificationStore()
const { smAndUp, mdAndUp } = useDisplay()

const loading = ref(false)
const backups = ref<Backup[]>([])
const connections = ref<Connection[]>([])

const filters = reactive({
  connectionId: null as number | null,
  status: null as BackupStatus | null,
  search: '',
})

const desktopHeaders = [
  { title: 'Conexão', key: 'connection', sortable: false },
  { title: 'Status', key: 'status', sortable: true },
  { title: 'Tamanho', key: 'fileSize', sortable: true },
  { title: 'Duração', key: 'duration', sortable: false },
  { title: 'Retenção', key: 'retentionType', sortable: true },
  { title: 'Tipo', key: 'trigger', sortable: false, align: 'center' as const },
  { title: 'Data', key: 'createdAt', sortable: true },
  { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
]

const mobileHeaders = [
  { title: 'Backup', key: 'connection', sortable: false },
  { title: 'Status', key: 'status', sortable: true },
  { title: 'Data', key: 'createdAt', sortable: true },
  { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
]

const tableHeaders = computed(() => (mdAndUp.value ? desktopHeaders : mobileHeaders))

const statusOptions = backupStatusOptions

async function loadBackups() {
  loading.value = true
  try {
    const response = await backupsApi.list({
      connectionId: filters.connectionId || undefined,
      status: filters.status || undefined,
    })
    backups.value = (response.data?.data ?? []).map((backup) => ({
      ...backup,
      protected: Boolean((backup as Backup & { protected: unknown }).protected),
    }))
  } catch (error) {
    console.error('Erro ao carregar backups:', error)
    notify('Erro ao carregar backups', 'error')
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

// Download
const downloadingId = ref<number | null>(null)

async function downloadBackup(backup: Backup) {
  if (!backup.fileName) {
    notify('Arquivo de backup não disponível', 'error')
    return
  }

  downloadingId.value = backup.id
  try {
    await backupsApi.download(backup.id, backup.fileName)
    notify('Download iniciado', 'success')
  } catch (error) {
    console.error('Erro ao fazer download:', error)
    notify('Erro ao fazer download do backup', 'error')
  } finally {
    downloadingId.value = null
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
    notify('Backup excluído com sucesso', 'success')
    deleteDialog.value = false
    loadBackups()
  } catch {
    notify('Erro ao excluir backup', 'error')
  } finally {
    deleteLoading.value = false
  }
}

// Helpers

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

/**
 * Handler para notificações de backup
 * Atualiza a lista automaticamente quando um backup é concluído ou falha
 */
function handleBackupNotification() {
  // Pequeno delay para garantir que o backend já persistiu as alterações
  setTimeout(() => {
    loadBackups()
  }, 500)
}

onMounted(() => {
  loadConnections()
  loadBackups()

  // Registra listener para atualizar lista quando backup terminar
  notificationStore.onNotification('backup', handleBackupNotification)
})

onUnmounted(() => {
  // Remove listener para evitar memory leaks
  notificationStore.offNotification('backup', handleBackupNotification)
})
</script>
