<template>
  <div>
    <!-- Not Running Warning -->
    <v-alert
      v-if="!isRunning"
      class="mb-4"
      density="comfortable"
      icon="mdi-alert-circle-outline"
      type="warning"
      variant="tonal"
    >
      O container não está em execução. Inicie o container para visualizar os processos ativos e o diagnóstico de CPU e memória em tempo real.
    </v-alert>

    <!-- Error Alert -->
    <v-alert
      v-else-if="requestError"
      class="mb-4"
      closable
      density="comfortable"
      icon="mdi-alert"
      type="error"
      variant="tonal"
      @click:close="requestError = null"
    >
      {{ requestError }}
    </v-alert>

    <!-- Live Resource Diagnostic Cards -->
    <v-row v-if="isRunning && resources" class="mb-4" dense>
      <!-- CPU Card -->
      <v-col cols="12" md="4" sm="6">
        <v-card class="process-metric-card h-100" variant="outlined">
          <v-card-text class="d-flex align-center justify-space-between py-3">
            <div>
              <div class="text-caption text-medium-emphasis font-weight-medium mb-1">
                <v-icon class="mr-1" color="primary" icon="mdi-cpu-64-bit" size="small" />
                USO DE CPU
              </div>
              <div class="text-h6 font-weight-bold tabular-cell">
                {{ resources.cpuPercent.toFixed(1) }}%
              </div>
              <div class="text-caption text-medium-emphasis">
                {{ resources.cpuPercent > 100 ? `${(resources.cpuPercent / 100).toFixed(1)} núcleos` : 'Carga instantânea' }}
              </div>
            </div>
            <v-progress-circular
              :color="resolveUsageColor(resources.cpuPercent)"
              :model-value="Math.min(100, resources.cpuPercent)"
              :rotate="-90"
              :size="58"
              :width="6"
            >
              <span class="text-caption font-weight-bold tabular-cell">
                {{ resources.cpuPercent.toFixed(0) }}%
              </span>
            </v-progress-circular>
          </v-card-text>
        </v-card>
      </v-col>

      <!-- Memory Card -->
      <v-col cols="12" md="4" sm="6">
        <v-card class="process-metric-card h-100" variant="outlined">
          <v-card-text class="d-flex align-center justify-space-between py-3">
            <div>
              <div class="text-caption text-medium-emphasis font-weight-medium mb-1">
                <v-icon class="mr-1" color="primary" icon="mdi-memory" size="small" />
                USO DE MEMÓRIA
              </div>
              <div class="text-h6 font-weight-bold tabular-cell">
                {{ formatBytes(resources.memoryUsageBytes) }}
              </div>
              <div class="text-caption text-medium-emphasis">
                {{ resources.memoryUsagePercent.toFixed(1) }}% de {{ formatBytes(resources.memoryLimitBytes) }}
              </div>
            </div>
            <v-progress-circular
              :color="resolveUsageColor(resources.memoryUsagePercent)"
              :model-value="Math.min(100, resources.memoryUsagePercent)"
              :rotate="-90"
              :size="58"
              :width="6"
            >
              <span class="text-caption font-weight-bold tabular-cell">
                {{ resources.memoryUsagePercent.toFixed(0) }}%
              </span>
            </v-progress-circular>
          </v-card-text>
        </v-card>
      </v-col>

      <!-- Processes / PIDs Card -->
      <v-col cols="12" md="4" sm="12">
        <v-card class="process-metric-card h-100" variant="outlined">
          <v-card-text class="d-flex align-center justify-space-between py-3">
            <div>
              <div class="text-caption text-medium-emphasis font-weight-medium mb-1">
                <v-icon class="mr-1" color="primary" icon="mdi-cogs" size="small" />
                PROCESSOS ATIVOS
              </div>
              <div class="text-h6 font-weight-bold tabular-cell">
                {{ processCount }}
              </div>
              <div class="text-caption text-medium-emphasis">
                <v-chip
                  class="mt-1"
                  color="success"
                  density="compact"
                  label
                  size="x-small"
                  variant="tonal"
                >
                  Em execução
                </v-chip>
              </div>
            </div>
            <v-avatar color="primary" rounded="lg" size="48" variant="tonal">
              <v-icon icon="mdi-format-list-numbered" size="24" />
            </v-avatar>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <!-- Resource History Charts (CPU & Memory) -->
    <ContainerResourceHistoryCard
      class="mb-4"
      :container-id="containerId"
      :is-running="isRunning"
    />

    <!-- Toolbar & Filters -->
    <div class="d-flex align-center flex-wrap ga-2 mb-3">
      <!-- Search filter -->
      <v-text-field
        v-model="searchQuery"
        clearable
        density="compact"
        hide-details
        placeholder="Filtrar por PID, usuário, comando..."
        prepend-inner-icon="mdi-magnify"
        style="max-width: 320px"
        variant="outlined"
      />

      <!-- PS Arguments preset -->
      <v-select
        v-model="psArgs"
        density="compact"
        hide-details
        :items="PS_OPTIONS"
        label="Argumentos ps"
        style="max-width: 140px"
        variant="outlined"
        @update:model-value="() => load(false)"
      />

      <!-- Auto-refresh interval -->
      <v-select
        v-model="autoRefreshInterval"
        density="compact"
        hide-details
        :items="REFRESH_OPTIONS"
        label="Atualização automática"
        style="max-width: 170px"
        variant="outlined"
      />

      <v-spacer />

      <!-- Copy Diagnostic -->
      <v-btn
        density="compact"
        :disabled="!isRunning || processes.length === 0"
        prepend-icon="mdi-content-copy"
        variant="tonal"
        @click="handleCopyDiagnostic"
      >
        Copiar diagnóstico
      </v-btn>

      <!-- Manual Refresh -->
      <v-btn
        color="primary"
        density="compact"
        :disabled="!isRunning"
        :loading="loading"
        prepend-icon="mdi-refresh"
        variant="tonal"
        @click="load(false)"
      >
        Atualizar
      </v-btn>
    </div>

    <!-- Reusable Process Table Component -->
    <ContainerProcessTable
      :empty-text="isRunning ? 'Nenhum processo encontrado com o filtro atual.' : 'Container inativo.'"
      :last-updated="lastUpdated"
      :loading="loading"
      :processes="filteredProcesses"
      :sort-column-index="sortColumnIndex"
      :sort-desc="sortDesc"
      :titles="titles"
      :total-count="processes.length"
      @sort="handleSort"
    />
  </div>
</template>

<script lang="ts" setup>
import { useNotifier } from '@/composables/useNotifier'
import { useContainerProcesses } from '@/composables/useContainerProcesses'
import { resolveUsageColor } from '@/utils/dockerMetrics'
import { formatBytes } from '@/utils/format'
import ContainerResourceHistoryCard from '@/components/docker/ContainerResourceHistoryCard.vue'
import ContainerProcessTable from '@/components/docker/ContainerProcessTable.vue'

const PS_OPTIONS = [
  { title: 'aux (padrão)', value: 'aux' },
  { title: '-ef (completo)', value: '-ef' },
  { title: 'ax (simples)', value: 'ax' },
  { title: 'top', value: 'top' },
]

const REFRESH_OPTIONS = [
  { title: 'Manual', value: 0 },
  { title: 'A cada 3s', value: 3000 },
  { title: 'A cada 5s', value: 5000 },
  { title: 'A cada 10s', value: 10000 },
  { title: 'A cada 30s', value: 30000 },
]

const props = defineProps<{
  containerId: string
  isRunning?: boolean
}>()

const notify = useNotifier()

const {
  titles,
  processes,
  resources,
  loading,
  requestError,
  lastUpdated,
  psArgs,
  searchQuery,
  sortColumnIndex,
  sortDesc,
  autoRefreshInterval,
  processCount,
  filteredProcesses,
  handleSort,
  load,
  copyDiagnosticText,
} = useContainerProcesses(
  () => props.containerId,
  () => props.isRunning
)

async function handleCopyDiagnostic(): Promise<void> {
  const result = await copyDiagnosticText()
  if (result) {
    notify('Diagnóstico e lista de processos copiados para a área de transferência!', 'success')
  } else {
    notify('Não foi possível copiar para a área de transferência.', 'error')
  }
}
</script>

<style scoped>
.process-metric-card {
  border-radius: 8px;
  background: linear-gradient(
    135deg,
    rgb(var(--v-theme-surface)) 0%,
    rgb(var(--v-theme-surface-bright)) 100%
  );
  border: 1px solid rgba(var(--v-border-color), 0.12);
}

.tabular-cell {
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum";
}
</style>
