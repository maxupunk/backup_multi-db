<template>
  <v-dialog v-model="isOpen" max-width="640" persistent scrollable>
    <v-card v-if="backup">
      <!-- Header with step indicator -->
      <v-card-title class="d-flex align-center justify-space-between pa-4">
        <div class="d-flex align-center gap-2">
          <v-icon color="warning" icon="mdi-backup-restore" />
          <span>Restaurar Backup</span>
        </div>
        <v-chip :color="step === 1 ? 'primary' : 'warning'" size="small" variant="tonal">
          Etapa {{ step }} de 2
        </v-chip>
      </v-card-title>

      <v-divider />

      <!-- ── Etapa 1: Configuração ── -->
      <v-card-text v-show="step === 1" class="pa-4">
        <!-- Backup de origem -->
        <div class="mb-4">
          <div class="text-caption text-medium-emphasis mb-1">Backup de origem</div>
          <div class="font-weight-medium">{{ backup.fileName }}</div>
          <div class="text-caption text-medium-emphasis">
            {{ backup.connection?.name }} &bull; {{ backup.databaseName }}
            <span v-if="backup.fileSize"> &bull; {{ formatFileSize(backup.fileSize) }}</span>
          </div>
        </div>

        <v-alert class="mb-4" color="info" density="compact" icon="mdi-shield-check" variant="tonal">
          Se o banco de destino já existir, um <strong>backup de segurança</strong> será criado
          automaticamente antes de restaurar.
        </v-alert>

        <v-divider class="mb-4" />

        <!-- Destino -->
        <div class="text-subtitle-2 mb-3">Destino</div>

        <v-select
          v-model="form.targetConnectionId"
          class="mb-3"
          density="comfortable"
          hide-details="auto"
          :items="availableConnections"
          item-title="name"
          item-value="id"
          label="Conexão de destino"
          variant="outlined"
        >
          <template #item="{ props: itemProps, item: connItem }">
            <v-list-item v-bind="itemProps">
              <template #prepend>
                <v-chip :color="getDatabaseColor(connItem.raw.type)" class="mr-2" label size="x-small">
                  {{ connItem.raw.type.toUpperCase() }}
                </v-chip>
              </template>
            </v-list-item>
          </template>
          <template #selection="{ item: connItem }">
            <div class="d-flex align-center gap-2">
              <v-chip :color="getDatabaseColor(connItem.raw.type)" label size="x-small">
                {{ connItem.raw.type.toUpperCase() }}
              </v-chip>
              <span>{{ connItem.raw.name }}</span>
            </div>
          </template>
        </v-select>

        <!-- Database de destino (com botão criar dentro do campo) -->
        <v-combobox
          v-model="form.targetDatabase"
          class="mb-4"
          clearable
          density="comfortable"
          hide-details="auto"
          hint="Selecione ou digite o nome do banco de destino"
          :items="targetDatabaseItems"
          label="Database de destino"
          :loading="loadingDatabases"
          no-data-text="Nenhum banco de dados encontrado na conexão"
          persistent-hint
          persistent-placeholder
          :placeholder="backup.databaseName"
          variant="outlined"
        >
          <template #append>
            <v-btn
              color="primary"
              :disabled="!form.targetConnectionId"
              icon
              size="small"
              variant="tonal"
              @click.stop="createDbDialog = true"
              @mousedown.stop
            >
              <v-icon icon="mdi-database-plus-outline" size="16" />
              <v-tooltip activator="parent" location="top">Criar novo banco de dados</v-tooltip>
            </v-btn>
          </template>
        </v-combobox>

        <v-divider class="mb-4" />

        <!-- Modo de restauração -->
        <div class="text-subtitle-2 mb-2">Modo de Restauração</div>
        <v-radio-group v-model="form.mode" class="mb-4" density="compact" hide-details>
          <v-radio label="Completo (Schema + Dados)" value="full" />
          <v-radio label="Apenas Schema (estrutura)" value="schema-only" />
          <v-radio label="Apenas Dados" value="data-only" />
        </v-radio-group>

        <!-- Opções PostgreSQL -->
        <template v-if="targetDbType === 'postgresql'">
          <v-divider class="mb-3" />
          <div class="text-subtitle-2 mb-1">Opções PostgreSQL</div>
          <v-checkbox v-model="form.noOwner" density="compact" hide-details
            label="Não restaurar owner (ALTER ... OWNER TO)" />
          <v-checkbox v-model="form.noPrivileges" density="compact" hide-details
            label="Não restaurar privilégios (GRANT / REVOKE)" />
          <v-checkbox v-model="form.noTablespaces" density="compact" hide-details
            label="Não restaurar tablespaces" />
          <v-checkbox v-model="form.noComments" density="compact" hide-details
            label="Não restaurar comentários (COMMENT ON)" />
        </template>

        <!-- Opções MySQL / MariaDB -->
        <template v-if="targetDbType === 'mysql' || targetDbType === 'mariadb'">
          <v-divider class="mb-3" />
          <div class="text-subtitle-2 mb-1">Opções MySQL / MariaDB</div>
          <v-checkbox v-model="form.noCreateDb" density="compact" hide-details
            label="Não executar CREATE DATABASE / USE" />
        </template>

        <v-divider class="my-3" />

        <v-checkbox
          v-model="form.clearBeforeRestore"
          color="warning"
          density="compact"
          hide-details
          label="Limpar banco de destino antes de restaurar (remove todos os dados existentes)"
        />
        <v-checkbox
          v-model="form.skipSafetyBackup"
          color="error"
          density="compact"
          hide-details
          label="Pular backup de segurança (não recomendado)"
        />
      </v-card-text>

      <!-- ── Etapa 2: Confirmação ── -->
      <v-card-text v-show="step === 2" class="pa-4">
        <v-alert class="mb-4" color="error" icon="mdi-alert" title="Atenção — operação destrutiva" variant="tonal">
          Esta operação irá <strong>sobrescrever todos os dados</strong> no banco de destino. Esta ação
          <strong>não pode ser desfeita</strong>.
        </v-alert>

        <!-- Resumo da operação -->
        <v-card class="mb-4" color="surface-variant" variant="tonal">
          <v-card-text class="pa-3">
            <div class="text-caption text-medium-emphasis mb-1">De (backup)</div>
            <div class="d-flex align-center gap-1 mb-3">
              <v-chip
                :color="getDatabaseColor(backup.connection?.type ?? 'postgresql')"
                label
                size="x-small"
              >
                {{ (backup.connection?.type ?? '').toUpperCase() }}
              </v-chip>
              <span class="font-weight-medium">{{ backup.connection?.name }}</span>
              <v-icon icon="mdi-chevron-right" size="14" />
              <v-icon icon="mdi-database-outline" size="14" />
              <span class="text-body-2">{{ backup.databaseName }}</span>
            </div>

            <div class="text-caption text-medium-emphasis mb-1">Para (destino)</div>
            <div class="d-flex align-center gap-1 mb-3">
              <v-chip
                :color="getDatabaseColor(targetDbType ?? 'postgresql')"
                label
                size="x-small"
              >
                {{ (targetDbType ?? '').toUpperCase() }}
              </v-chip>
              <span class="font-weight-medium">{{ selectedConnection?.name }}</span>
              <v-icon icon="mdi-chevron-right" size="14" />
              <v-icon icon="mdi-database-outline" size="14" />
              <span class="text-body-2 font-weight-bold">{{ targetDatabaseName }}</span>
            </div>

            <div class="text-caption text-medium-emphasis mb-1">Modo</div>
            <v-chip color="warning" size="small" variant="tonal">
              {{ modeLabelMap[form.mode] }}
            </v-chip>
          </v-card-text>
        </v-card>

        <!-- Campo de confirmação -->
        <div class="text-body-2 mb-2">
          Para confirmar, digite o nome do banco de destino:
          <code class="font-weight-bold">{{ targetDatabaseName }}</code>
        </div>
        <v-text-field
          v-model="confirmationInput"
          autocomplete="off"
          density="comfortable"
          :error="confirmationInput.length > 0 && !isConfirmationValid"
          hide-details="auto"
          :hint="isConfirmationValid ? 'Confirmação válida' : ''"
          label="Confirmação"
          persistent-hint
          variant="outlined"
        />
      </v-card-text>

      <v-divider />

      <!-- Ações -->
      <v-card-actions class="pa-3">
        <v-btn :disabled="restoreLoading" variant="text" @click="close()">
          Cancelar
        </v-btn>

        <v-spacer />

        <!-- Etapa 1: avançar -->
        <v-btn
          v-if="step === 1"
          color="warning"
          :disabled="!isStep1Valid"
          variant="flat"
          @click="goToConfirmation()"
        >
          Próximo
          <v-icon end icon="mdi-arrow-right" />
        </v-btn>

        <!-- Etapa 2: voltar + confirmar -->
        <template v-else>
          <v-btn :disabled="restoreLoading" variant="tonal" @click="goBack()">
            <v-icon start icon="mdi-arrow-left" />
            Voltar
          </v-btn>
          <v-btn
            class="ml-2"
            color="error"
            :disabled="!isConfirmationValid"
            :loading="restoreLoading"
            variant="flat"
            @click="execute()"
          >
            <v-icon start icon="mdi-backup-restore" />
            Restaurar agora
          </v-btn>
        </template>
      </v-card-actions>
    </v-card>
  </v-dialog>

  <!-- CreateDatabaseDialog: fica aqui dentro para encapsular toda a lógica de restore -->
  <CreateDatabaseDialog
    v-model="createDbDialog"
    :connection-id="form.targetConnectionId"
    :connection-name="selectedConnection?.name ?? ''"
    :placeholder="backup?.databaseName"
    @created="addDatabase"
  />
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import type { Backup, Connection } from '@/types/api'
import { useRestoreDialog } from '@/composables/useRestoreDialog'
import CreateDatabaseDialog from '@/components/common/CreateDatabaseDialog.vue'
import { getDatabaseColor } from '@/ui/database'
import { formatFileSize } from '@/utils/format'

// ── Emits ─────────────────────────────────────────────────────────────────────

const emit = defineEmits<{
  /** Disparado após o restore ser enviado com sucesso */
  success: []
}>()

// ── Composable (SRP: toda lógica de restore encapsulada aqui) ──────────────────

const {
  isOpen,
  step,
  backup,
  form,
  restoreLoading,
  availableConnections,
  targetDatabaseItems,
  loadingDatabases,
  confirmationInput,
  selectedConnection,
  targetDatabaseName,
  targetDbType,
  isConfirmationValid,
  open,
  close,
  goToConfirmation,
  goBack,
  addDatabase,
  execute,
} = useRestoreDialog(() => emit('success'))

// ── Estado local ──────────────────────────────────────────────────────────────

const createDbDialog = ref(false)

// ── Computados ────────────────────────────────────────────────────────────────

const isStep1Valid = computed(
  () => !!form.targetConnectionId && !!targetDatabaseName.value,
)

const modeLabelMap: Record<string, string> = {
  full: 'Completo',
  'schema-only': 'Apenas Schema',
  'data-only': 'Apenas Dados',
}

// ── API pública (chamada pelo pai via template ref) ────────────────────────────

defineExpose({ open })
</script>
