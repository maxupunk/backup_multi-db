<template>
  <div v-if="store.operations.length > 0" class="operation-progress-container">
    <v-card
      v-for="op in store.operations"
      :key="op.operationId"
      class="operation-progress-card mb-3"
      :class="getCardClass(op.stage)"
    >
      <v-card-text class="pa-4">
        <!-- Header -->
        <div class="d-flex align-center justify-space-between mb-3">
          <div class="d-flex align-center gap-2">
            <v-icon :color="getHeaderIconColor(op)" :icon="getHeaderIcon(op)" size="20" />
            <span class="text-subtitle-2 font-weight-bold">
              {{ getHeaderTitle(op) }}
            </span>
          </div>
          <v-btn
            v-if="isFinished(op.stage)"
            density="compact"
            icon="mdi-close"
            size="x-small"
            variant="text"
            @click="store.dismiss(op.operationId)"
          />
          <v-chip v-else color="info" label size="x-small" variant="flat">
            <v-icon class="mr-1" icon="mdi-loading mdi-spin" size="12" />
            Em andamento
          </v-chip>
        </div>

        <!-- Database info -->
        <div class="text-body-2 mb-3">
          <span class="text-medium-emphasis">Conexão:</span>
          <span class="font-weight-medium ml-1">{{ op.connectionName }}</span>
          <span class="mx-2 text-medium-emphasis">&bull;</span>
          <span class="text-medium-emphasis">Database:</span>
          <span class="font-weight-medium ml-1">{{ op.databaseName }}</span>
        </div>

        <!-- Stages Timeline -->
        <div class="stages-timeline mb-3">
          <div
            v-for="stageDef in getStagesForOp(op)"
            :key="stageDef.key"
            class="stage-item d-flex align-center mb-1"
          >
            <v-icon
              class="mr-2 flex-shrink-0"
              :color="getStageIconColor(stageDef.key, op)"
              :icon="getStageIcon(stageDef.key, op)"
              size="18"
            />
            <span
              class="text-body-2"
              :class="{
                'font-weight-medium': isCurrentStage(stageDef.key, op),
                'text-medium-emphasis': !isCurrentStage(stageDef.key, op) && !isStageCompleted(stageDef.key, op),
              }"
            >
              {{ stageDef.label }}
            </span>
            <v-progress-circular
              v-if="isCurrentStage(stageDef.key, op) && !isFinished(op.stage)"
              class="ml-2"
              color="primary"
              indeterminate
              size="14"
              width="2"
            />
          </div>
        </div>

        <!-- Progress bar (restore com percentual) -->
        <div v-if="showPercentBar(op)">
          <div class="d-flex align-center justify-space-between mb-1">
            <span class="text-caption text-medium-emphasis">Progresso</span>
            <span class="text-caption font-weight-bold">{{ Math.round(op.progress) }}%</span>
          </div>
          <v-progress-linear
            :color="op.stage === 'completed' ? 'success' : 'primary'"
            height="8"
            :model-value="op.progress"
            rounded
          />
        </div>

        <!-- Progress info (backup com bytes escritos) -->
        <div v-else-if="showBytesInfo(op)">
          <v-progress-linear color="primary" height="8" indeterminate rounded />
        </div>

        <!-- Current message -->
        <div class="d-flex align-center mt-2">
          <v-icon
            class="mr-1"
            :color="getMessageColor(op.stage)"
            :icon="getMessageIcon(op.stage)"
            size="14"
          />
          <span class="text-caption" :class="getMessageClass(op.stage)">
            {{ op.message }}
          </span>
        </div>

        <!-- Error details -->
        <v-alert
          v-if="op.stage === 'failed' && op.error"
          class="mt-3"
          color="error"
          density="compact"
          icon="mdi-alert-circle-outline"
          variant="tonal"
        >
          <div class="text-caption" style="word-break: break-all;">{{ op.error }}</div>
        </v-alert>
      </v-card-text>
    </v-card>
  </div>
</template>

<script lang="ts" setup>
import {
  getStageOrder,
  getStages,
  useOperationProgressStore,
  type ActiveOperation,
  type StageDefinition,
  type StageKey,
} from '@/stores/operation-progress'

const store = useOperationProgressStore()

function isFinished(stage: StageKey): boolean {
  return stage === 'completed' || stage === 'failed'
}

function isCurrentStage(stageKey: StageKey, op: ActiveOperation): boolean {
  return op.stage === stageKey
}

function getStagesForOp(op: ActiveOperation): StageDefinition[] {
  return getStages(op.type)
}

function isStageCompleted(stageKey: StageKey, op: ActiveOperation): boolean {
  if (op.stage === 'completed') return true
  if (op.stage === 'failed') return false
  const order = getStageOrder(op.type)
  return (order[stageKey] ?? 0) < (order[op.stage] ?? 0)
}

function getStageIcon(stageKey: StageKey, op: ActiveOperation): string {
  if (isStageCompleted(stageKey, op)) return 'mdi-check-circle'
  if (isCurrentStage(stageKey, op)) {
    const stages = getStages(op.type)
    return stages.find((s) => s.key === stageKey)?.icon ?? 'mdi-circle-outline'
  }
  return 'mdi-circle-outline'
}

function getStageIconColor(stageKey: StageKey, op: ActiveOperation): string {
  if (op.stage === 'failed' && isCurrentStage(stageKey, op)) return 'error'
  if (isStageCompleted(stageKey, op)) return 'success'
  if (isCurrentStage(stageKey, op)) return 'primary'
  return 'grey'
}

function getCardClass(stage: StageKey): string {
  if (stage === 'completed') return 'operation-card--success'
  if (stage === 'failed') return 'operation-card--error'
  return 'operation-card--active'
}

function getHeaderIcon(op: ActiveOperation): string {
  if (op.stage === 'completed') return 'mdi-check-circle'
  if (op.stage === 'failed') return 'mdi-alert-circle'
  return op.type === 'backup' ? 'mdi-database-export-outline' : 'mdi-backup-restore'
}

function getHeaderIconColor(op: ActiveOperation): string {
  if (op.stage === 'completed') return 'success'
  if (op.stage === 'failed') return 'error'
  return 'primary'
}

function getHeaderTitle(op: ActiveOperation): string {
  const label = op.type === 'backup' ? 'Backup' : 'Restauração'
  if (op.stage === 'completed') return `${label} Concluído`
  if (op.stage === 'failed') return `${label} Falhou`
  return op.type === 'backup' ? 'Fazendo Backup' : 'Restaurando Backup'
}

function showPercentBar(op: ActiveOperation): boolean {
  if (op.type === 'restore') {
    return op.stage === 'restoring' || (op.stage === 'completed' && op.progress > 0)
  }
  return false
}

function showBytesInfo(op: ActiveOperation): boolean {
  if (op.type === 'backup') {
    return op.stage === 'dumping' || op.stage === 'compressing' || op.stage === 'uploading'
  }
  return false
}

function getMessageIcon(stage: StageKey): string {
  if (stage === 'completed') return 'mdi-check'
  if (stage === 'failed') return 'mdi-alert'
  return 'mdi-information-outline'
}

function getMessageColor(stage: StageKey): string {
  if (stage === 'completed') return 'success'
  if (stage === 'failed') return 'error'
  return 'info'
}

function getMessageClass(stage: StageKey): string {
  if (stage === 'completed') return 'text-success'
  if (stage === 'failed') return 'text-error'
  return 'text-medium-emphasis'
}
</script>

<style scoped>
.operation-progress-container {
  position: fixed;
  bottom: 16px;
  right: 16px;
  z-index: 1500;
  width: 420px;
  max-width: calc(100vw - 32px);
}

.operation-progress-card {
  box-shadow: 0 4px 25px rgba(0, 0, 0, 0.2) !important;
  border-radius: 12px !important;
}

.operation-card--active {
  background: rgb(var(--v-theme-surface)) !important;
  border-left: 4px solid rgb(var(--v-theme-primary));
}

.operation-card--success {
  background: rgb(var(--v-theme-surface)) !important;
  border-left: 4px solid rgb(var(--v-theme-success));
}

.operation-card--error {
  background: rgb(var(--v-theme-surface)) !important;
  border-left: 4px solid rgb(var(--v-theme-error));
}

.stages-timeline {
  position: relative;
}

.stage-item {
  position: relative;
  min-height: 28px;
}

@media (max-width: 600px) {
  .operation-progress-container {
    bottom: 8px;
    right: 8px;
    left: 8px;
    width: auto;
  }
}
</style>
