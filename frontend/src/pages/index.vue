<template>
  <div>
    <!-- Header -->
    <div class="d-flex align-center justify-space-between mb-6">
      <div>
        <h1 class="text-h4 font-weight-bold mb-1">Dashboard</h1>
        <p class="text-body-2 text-medium-emphasis">
          Visão geral do sistema de backups
        </p>
      </div>

      <v-btn color="primary" :loading="loading" prepend-icon="mdi-refresh" @click="loadStats">
        Atualizar
      </v-btn>
    </div>

    <!-- Stats Cards -->
    <v-row class="mb-6">
      <v-col cols="12" md="3" sm="6">
        <v-card class="stat-card">
          <v-card-text class="d-flex align-center pa-4">
            <v-avatar class="mr-4" color="primary" size="56">
              <v-icon icon="mdi-database" size="28" />
            </v-avatar>
            <div>
              <p class="text-caption text-medium-emphasis mb-1">
                Total Conexões
              </p>
              <p class="text-h4 font-weight-bold">
                {{ stats?.connections?.total ?? 0 }}
              </p>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card class="stat-card">
          <v-card-text class="d-flex align-center pa-4">
            <v-avatar class="mr-4" color="success" size="56">
              <v-icon icon="mdi-check-network" size="28" />
            </v-avatar>
            <div>
              <p class="text-caption text-medium-emphasis mb-1">
                Conexões Ativas
              </p>
              <p class="text-h4 font-weight-bold">
                {{ stats?.connections?.active ?? 0 }}
              </p>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card class="stat-card">
          <v-card-text class="d-flex align-center pa-4">
            <v-avatar class="mr-4" color="info" size="56">
              <v-icon icon="mdi-backup-restore" size="28" />
            </v-avatar>
            <div>
              <p class="text-caption text-medium-emphasis mb-1">
                Total Backups
              </p>
              <p class="text-h4 font-weight-bold">
                {{ stats?.backups?.total ?? 0 }}
              </p>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="3" sm="6">
        <v-card class="stat-card">
          <v-card-text class="d-flex align-center pa-4">
            <v-avatar class="mr-4" color="secondary" size="56">
              <v-icon icon="mdi-calendar-today" size="28" />
            </v-avatar>
            <div>
              <p class="text-caption text-medium-emphasis mb-1">
                Backups Hoje
              </p>
              <p class="text-h4 font-weight-bold">
                {{ stats?.backups?.today ?? 0 }}
              </p>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <v-row>
      <!-- Quick Actions -->
      <v-col cols="12" md="4">
        <v-card>
          <v-card-title class="d-flex align-center">
            <v-icon class="mr-2" color="warning" icon="mdi-lightning-bolt" />
            Ações Rápidas
          </v-card-title>

          <v-card-text>
            <v-btn
              block
              class="mb-3"
              color="primary"
              prepend-icon="mdi-plus"
              to="/connections/new"
            >
              Nova Conexão
            </v-btn>

            <v-btn
              block
              color="primary"
              prepend-icon="mdi-backup-restore"
              to="/backups"
              variant="outlined"
            >
              Ver Todos os Backups
            </v-btn>
          </v-card-text>
        </v-card>
      </v-col>

      <!-- Recent Backups -->
      <v-col cols="12" md="8">
        <v-card>
          <v-card-title class="d-flex align-center justify-space-between">
            <div class="d-flex align-center">
              <v-icon class="mr-2" color="info" icon="mdi-history" />
              Backups Recentes
            </div>

            <v-btn size="small" to="/backups" variant="text">
              Ver todos
            </v-btn>
          </v-card-title>

          <v-card-text>
            <v-table v-if="stats?.recentBackups?.length" density="comfortable">
              <thead>
                <tr>
                  <th>Conexão</th>
                  <th>Status</th>
                  <th>Tamanho</th>
                  <th>Data</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="backup in stats.recentBackups" :key="backup.id">
                  <td class="font-weight-medium">
                    {{ backup.connectionName }}
                  </td>
                  <td>
                    <v-chip :color="getStatusColor(backup.status)" label size="small">
                      {{ getStatusLabel(backup.status) }}
                    </v-chip>
                  </td>
                  <td class="text-medium-emphasis">
                    {{ formatFileSize(backup.fileSize) }}
                  </td>
                  <td class="text-medium-emphasis">
                    {{ formatDate(backup.createdAt) }}
                  </td>
                </tr>
              </tbody>
            </v-table>

            <v-alert v-else class="text-center" type="info" variant="tonal">
              Nenhum backup realizado ainda.
              <br>
              <v-btn class="mt-2" color="primary" to="/connections" variant="text">
                Criar primeira conexão
              </v-btn>
            </v-alert>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>
  </div>
</template>

<script lang="ts" setup>
  import type { BackupStatus, DashboardStats } from '@/types/api'
  import { onMounted, ref } from 'vue'
  import { statsApi } from '@/services/api'
  import { useNotifier } from '@/composables/useNotifier'
  import { getBackupStatusColor as getStatusColor, getBackupStatusLabel as getStatusLabel } from '@/ui/backup'
  import { formatDateTimePtBR, formatFileSize } from '@/utils/format'

  const notify = useNotifier()

  const loading = ref(false)
  const stats = ref<DashboardStats | null>(null)

  async function loadStats () {
    loading.value = true
    try {
      const response = await statsApi.get()
      stats.value = response.data ?? null
    } catch (error) {
      console.error('Erro ao carregar estatísticas:', error)
      notify('Erro ao carregar estatísticas', 'error')
    } finally {
      loading.value = false
    }
  }

  function formatDate (dateString: string): string {
    return formatDateTimePtBR(dateString, { withYear: false })
  }

  onMounted(() => {
    loadStats()
  })
</script>

<style scoped>
.stat-card {
  background: linear-gradient(135deg,
      rgb(var(--v-theme-surface)) 0%,
      rgb(var(--v-theme-surface-bright)) 100%);
  border: 1px solid rgba(var(--v-border-color), 0.08);
  transition: transform 0.2s ease, box-shadow 0.2s ease;
}

.stat-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
}
</style>
