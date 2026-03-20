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
        <!-- Connection + Database -->
        <template #item.connection="{ item }">
          <div v-if="item.connection" class="d-flex flex-column gap-1 py-2">
            <!-- Linha 1: tipo + nome da conexão -->
            <div class="d-flex align-center gap-2">
              <v-chip :color="getDatabaseColor(item.connection.type)" label size="x-small" class="flex-shrink-0">
                {{ item.connection.type.toUpperCase() }}
              </v-chip>
              <span class="font-weight-medium text-body-2">{{ item.connection.name }}</span>
            </div>

            <!-- Linha 2: database -->
            <div class="d-flex align-center gap-1 pl-1">
              <v-icon icon="mdi-database-outline" size="13" color="medium-emphasis" />
              <span class="text-caption text-medium-emphasis">{{ item.databaseName }}</span>
            </div>

            <!-- Linha 3: badge segurança (condicional) -->
            <div v-if="item.metadata?.isRestoreSafetyBackup" class="pl-1">
              <v-chip color="teal" label size="x-small" prepend-icon="mdi-shield-check" variant="tonal">
                Backup de segurança
              </v-chip>
            </div>
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
          <template v-if="item.metadata?.isRestoreSafetyBackup">
            <v-icon color="teal" icon="mdi-shield-check" size="20" />
            <v-tooltip activator="parent" location="top">Backup de segurança (restauração)</v-tooltip>
          </template>
          <template v-else-if="item.trigger === 'scheduled'">
            <v-icon color="info" icon="mdi-clock-outline" size="20" />
            <v-tooltip activator="parent" location="top">Agendado</v-tooltip>
          </template>
          <template v-else>
            <v-icon color="warning" icon="mdi-hand-pointing-right" size="20" />
            <v-tooltip activator="parent" location="top">Manual</v-tooltip>
          </template>
        </template>

        <!-- Created At -->
        <template #item.createdAt="{ item }">
          {{ formatDate(item.createdAt) }}
        </template>

        <!-- Actions -->
        <template #item.actions="{ item }">
          <template v-if="mdAndUp">
            <v-btn color="secondary" icon="mdi-information-outline" size="small" variant="text"
              @click="openDetailDialog(item)">
              <v-icon icon="mdi-information-outline" />
              <v-tooltip activator="parent" location="top">Detalhes</v-tooltip>
            </v-btn>

            <v-btn v-if="item.connection" color="secondary" icon="mdi-open-in-new" size="small" variant="text"
              :to="`/connections/${item.connection.id}`">
              <v-icon icon="mdi-open-in-new" />
              <v-tooltip activator="parent" location="top">Ir para a conexão</v-tooltip>
            </v-btn>

            <v-btn v-if="item.status === 'completed' && item.filePath" color="primary" icon="mdi-download"
              :loading="downloadingId === item.id" size="small" variant="text" @click="downloadBackup(item)">
              <v-icon icon="mdi-download" />
              <v-tooltip activator="parent" location="top">Download</v-tooltip>
            </v-btn>

            <v-btn v-if="item.status === 'completed' && item.filePath" color="warning" icon="mdi-backup-restore"
              size="small" variant="text" @click="openRestoreDialog(item)">
              <v-icon icon="mdi-backup-restore" />
              <v-tooltip activator="parent" location="top">Restaurar</v-tooltip>
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
              <v-list-item prepend-icon="mdi-information-outline" title="Detalhes" @click="openDetailDialog(item)" />
              <v-list-item v-if="item.connection" prepend-icon="mdi-open-in-new" title="Ir para a conexão"
                :to="`/connections/${item.connection.id}`" />
              <v-list-item v-if="item.status === 'completed' && item.filePath" :disabled="downloadingId === item.id"
                prepend-icon="mdi-download" title="Download" @click="downloadBackup(item)" />
              <v-list-item v-if="item.status === 'completed' && item.filePath"
                prepend-icon="mdi-backup-restore" title="Restaurar" @click="openRestoreDialog(item)" />
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

    <!-- Backup Detail Dialog -->
    <v-dialog v-model="detailDialog" max-width="560" scrollable>
      <v-card v-if="backupDetail">
        <v-card-title class="d-flex align-center justify-space-between">
          <div class="d-flex align-center">
            <v-icon class="mr-2" color="secondary" icon="mdi-information-outline" />
            Detalhes do Backup
          </div>
          <v-btn icon="mdi-close" variant="text" @click="detailDialog = false" />
        </v-card-title>

        <v-divider />

        <v-card-text class="pa-4">
          <!-- Header: conexão + status -->
          <div class="d-flex align-center justify-space-between mb-4">
            <div class="d-flex align-center gap-2">
              <v-chip v-if="backupDetail.connection" :color="getDatabaseColor(backupDetail.connection.type)" label
                size="small">
                {{ backupDetail.connection.type.toUpperCase() }}
              </v-chip>
              <div>
                <div class="font-weight-medium">{{ backupDetail.connection?.name ?? 'N/A' }}</div>
                <div class="text-caption text-medium-emphasis">{{ backupDetail.connection?.host }}</div>
              </div>
            </div>
            <v-chip :color="getStatusColor(backupDetail.status)" label size="small">
              <v-icon class="mr-1" :icon="getStatusIcon(backupDetail.status)" size="14" />
              {{ getStatusLabel(backupDetail.status) }}
            </v-chip>
          </div>

          <v-divider class="mb-4" />

          <!-- Informações -->  
          <v-row dense>
            <v-col cols="6">
              <div class="text-caption text-medium-emphasis">Database</div>
              <div class="font-weight-medium d-flex align-center">
                <v-icon icon="mdi-database-outline" size="14" class="mr-1" />
                {{ backupDetail.databaseName }}
              </div>
            </v-col>

            <v-col cols="6">
              <div class="text-caption text-medium-emphasis">Retenção</div>
              <v-chip :color="getRetentionColor(backupDetail.retentionType)" size="x-small" variant="tonal">
                {{ getRetentionLabel(backupDetail.retentionType) }}
              </v-chip>
            </v-col>

            <v-col cols="6" class="mt-2">
              <div class="text-caption text-medium-emphasis">Tamanho</div>
              <div class="font-weight-medium">
                {{ backupDetail.fileSize ? formatFileSize(backupDetail.fileSize) : '—' }}
              </div>
            </v-col>

            <v-col cols="6" class="mt-2">
              <div class="text-caption text-medium-emphasis">Duração</div>
              <div class="font-weight-medium">
                {{ backupDetail.durationSeconds ? formatDuration(backupDetail.durationSeconds) : '—' }}
              </div>
            </v-col>

            <v-col cols="6" class="mt-2">
              <div class="text-caption text-medium-emphasis">Tipo</div>
              <div class="d-flex align-center">
                <template v-if="backupDetail.metadata?.isRestoreSafetyBackup">
                  <v-icon size="16" class="mr-1" color="teal" icon="mdi-shield-check" />
                  Segurança (restauração)
                </template>
                <template v-else-if="backupDetail.trigger === 'scheduled'">
                  <v-icon size="16" class="mr-1" color="info" icon="mdi-clock-outline" />
                  Agendado
                </template>
                <template v-else>
                  <v-icon size="16" class="mr-1" color="warning" icon="mdi-hand-pointing-right" />
                  Manual
                </template>
              </div>
            </v-col>

            <v-col cols="6" class="mt-2">
              <div class="text-caption text-medium-emphasis">Comprimido</div>
              <div class="d-flex align-center">
                <v-icon size="16" class="mr-1"
                  :color="backupDetail.compressed ? 'success' : 'grey'"
                  :icon="backupDetail.compressed ? 'mdi-check-circle-outline' : 'mdi-close-circle-outline'" />
                {{ backupDetail.compressed ? 'Sim' : 'Não' }}
              </div>
            </v-col>

            <v-col cols="12" class="mt-2">
              <div class="text-caption text-medium-emphasis">Arquivo</div>
              <div class="font-weight-medium text-body-2" style="word-break: break-all;">
                {{ backupDetail.fileName ?? '—' }}
              </div>
            </v-col>

            <v-col v-if="backupDetail.checksum" cols="12" class="mt-2">
              <div class="text-caption text-medium-emphasis">Checksum</div>
              <div class="font-mono text-caption" style="word-break: break-all;">
                {{ backupDetail.checksum }}
              </div>
            </v-col>

            <v-col cols="6" class="mt-2">
              <div class="text-caption text-medium-emphasis">Iniciado em</div>
              <div class="text-body-2">{{ backupDetail.startedAt ? formatDate(backupDetail.startedAt) : '—' }}</div>
            </v-col>

            <v-col cols="6" class="mt-2">
              <div class="text-caption text-medium-emphasis">Finalizado em</div>
              <div class="text-body-2">{{ backupDetail.finishedAt ? formatDate(backupDetail.finishedAt) : '—' }}</div>
            </v-col>
          </v-row>

          <!-- Erro -->
          <v-alert v-if="backupDetail.errorMessage" class="mt-4" color="error" density="compact"
            icon="mdi-alert-circle-outline" variant="tonal">
            <div class="text-caption font-weight-medium mb-1">Mensagem de erro</div>
            <div class="text-caption" style="word-break: break-all;">{{ backupDetail.errorMessage }}</div>
          </v-alert>
        </v-card-text>

        <v-divider />

        <v-card-actions>
          <v-btn v-if="backupDetail.connection" color="secondary" prepend-icon="mdi-open-in-new" variant="tonal"
            :to="`/connections/${backupDetail.connection.id}`">
            Ir para a conexão
          </v-btn>
          <v-spacer />
          <v-btn variant="text" @click="detailDialog = false">Fechar</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

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

    <!-- Restore Dialog -->
    <v-dialog v-model="restoreDialog" max-width="600" persistent>
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon class="mr-2" color="warning" icon="mdi-backup-restore" />
          Restaurar Backup
        </v-card-title>

        <v-card-text v-if="backupToRestore">
          <v-alert class="mb-3" color="info" density="compact" icon="mdi-shield-check" variant="tonal">
            Se o database de destino já existir, um <strong>backup de segurança</strong> será criado automaticamente
            antes de restaurar.
          </v-alert>

          <v-alert class="mb-4" color="warning" density="compact" icon="mdi-alert" variant="tonal">
            A restauração irá sobrescrever os dados existentes no banco de destino. Esta operação não pode ser desfeita.
          </v-alert>

          <div class="mb-4">
            <div class="text-body-2 text-medium-emphasis mb-1">Backup</div>
            <div class="font-weight-medium">{{ backupToRestore.fileName }}</div>
            <div class="text-caption text-medium-emphasis">
              {{ backupToRestore.connection?.name }} &bull; {{ backupToRestore.databaseName }}
              <span v-if="backupToRestore.fileSize"> &bull; {{ formatFileSize(backupToRestore.fileSize) }}</span>
            </div>
          </div>

          <v-divider class="mb-4" />

          <!-- Modo de restauração -->
          <div class="mb-4">
            <div class="text-subtitle-2 mb-2">Modo de Restauração</div>
            <v-radio-group v-model="restoreOptions.mode" density="compact" hide-details>
              <v-radio label="Completo (Schema + Dados)" value="full" />
              <v-radio label="Apenas Schema (estrutura)" value="schema-only" />
              <v-radio label="Apenas Dados" value="data-only" />
            </v-radio-group>
          </div>

          <!-- Database de destino -->
          <v-text-field
            v-model="restoreOptions.targetDatabase"
            class="mb-2"
            clearable
            density="comfortable"
            hide-details
            hint="Deixe vazio para usar o database original"
            label="Database de destino (opcional)"
            persistent-hint
            :placeholder="backupToRestore.databaseName"
            variant="outlined"
          />

          <v-divider class="my-4" />

          <!-- Opções PostgreSQL -->
          <template v-if="restoreDbType === 'postgresql'">
            <div class="text-subtitle-2 mb-2">Opções PostgreSQL</div>
            <v-checkbox
              v-model="restoreOptions.noOwner"
              density="compact"
              hide-details
              label="Não restaurar owner (ALTER ... OWNER TO)"
            />
            <v-checkbox
              v-model="restoreOptions.noPrivileges"
              density="compact"
              hide-details
              label="Não restaurar privilégios (GRANT / REVOKE)"
            />
            <v-checkbox
              v-model="restoreOptions.noTablespaces"
              density="compact"
              hide-details
              label="Não restaurar tablespaces"
            />
            <v-checkbox
              v-model="restoreOptions.noComments"
              density="compact"
              hide-details
              label="Não restaurar comentários (COMMENT ON)"
            />
          </template>

          <!-- Opções MySQL/MariaDB -->
          <template v-if="restoreDbType === 'mysql' || restoreDbType === 'mariadb'">
            <div class="text-subtitle-2 mb-2">Opções MySQL / MariaDB</div>
            <v-checkbox
              v-model="restoreOptions.noCreateDb"
              density="compact"
              hide-details
              label="Não executar CREATE DATABASE / USE"
            />
          </template>

          <v-divider class="my-4" />

          <!-- Opções avançadas -->
          <v-checkbox
            v-model="restoreOptions.skipSafetyBackup"
            color="error"
            density="compact"
            hide-details
            label="Pular backup de segurança (não recomendado)"
          />
        </v-card-text>

        <v-card-actions>
          <v-spacer />
          <v-btn :disabled="restoreLoading" variant="text" @click="restoreDialog = false">
            Cancelar
          </v-btn>
          <v-btn color="warning" :loading="restoreLoading" variant="flat" @click="restoreBackup">
            <v-icon class="mr-1" icon="mdi-backup-restore" />
            Restaurar
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script lang="ts" setup>
import type { Backup, BackupStatus, Connection, DatabaseType, RestoreMode, RetentionType } from '@/types/api'
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

// Detail
const detailDialog = ref(false)
const backupDetail = ref<Backup | null>(null)

function openDetailDialog(backup: Backup) {
  backupDetail.value = backup
  detailDialog.value = true
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

// Restore
const restoreDialog = ref(false)
const restoreLoading = ref(false)
const backupToRestore = ref<Backup | null>(null)

const restoreOptions = reactive({
  mode: 'full' as RestoreMode,
  targetDatabase: '' as string,
  noOwner: false,
  noPrivileges: false,
  noTablespaces: false,
  noComments: false,
  noCreateDb: false,
  skipSafetyBackup: false,
})

const restoreDbType = computed(() => backupToRestore.value?.connection?.type ?? null)

function openRestoreDialog(backup: Backup) {
  backupToRestore.value = backup
  // Resetar opções
  restoreOptions.mode = 'full'
  restoreOptions.targetDatabase = ''
  restoreOptions.noOwner = false
  restoreOptions.noPrivileges = false
  restoreOptions.noTablespaces = false
  restoreOptions.noComments = false
  restoreOptions.noCreateDb = false
  restoreOptions.skipSafetyBackup = false
  restoreDialog.value = true
}

async function restoreBackup() {
  if (!backupToRestore.value) return

  restoreLoading.value = true
  try {
    const payload: Record<string, unknown> = {
      mode: restoreOptions.mode,
      skipSafetyBackup: restoreOptions.skipSafetyBackup || undefined,
    }
    if (restoreOptions.targetDatabase) {
      payload.targetDatabase = restoreOptions.targetDatabase
    }
    if (restoreDbType.value === 'postgresql') {
      if (restoreOptions.noOwner) payload.noOwner = true
      if (restoreOptions.noPrivileges) payload.noPrivileges = true
      if (restoreOptions.noTablespaces) payload.noTablespaces = true
      if (restoreOptions.noComments) payload.noComments = true
    }
    if ((restoreDbType.value === 'mysql' || restoreDbType.value === 'mariadb') && restoreOptions.noCreateDb) {
      payload.noCreateDb = true
    }

    const response = await backupsApi.restore(backupToRestore.value.id, payload as any)
    const safetyBackup = response.data?.safetyBackup

    if (safetyBackup?.success) {
      notify(
        `Backup de segurança criado: #${safetyBackup.id} (${safetyBackup.fileName ?? 'arquivo'})`,
        'info'
      )
    }

    notify(response.message ?? 'Backup restaurado com sucesso', 'success')
    restoreDialog.value = false
    loadBackups()
  } catch (error: any) {
    const msg = error?.message ?? 'Erro ao restaurar backup'
    notify(msg, 'error')
  } finally {
    restoreLoading.value = false
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
