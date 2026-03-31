<template>
  <v-dialog v-model="isOpen" max-width="600" persistent scrollable>
    <v-card>
      <v-card-title class="d-flex align-center justify-space-between pa-4">
        <div class="d-flex align-center gap-2">
          <v-icon color="primary" icon="mdi-database-import-outline" />
          <span>Importar Backup</span>
        </div>
        <v-btn :disabled="uploading" icon="mdi-close" variant="text" @click="close" />
      </v-card-title>

      <v-divider />

      <!-- ── Etapa 1: Upload ── -->
      <v-card-text v-if="step === 'upload'" class="pa-4">
        <!-- Formatos aceitos -->
        <v-alert class="mb-4" color="info" density="compact" icon="mdi-information-outline" variant="tonal">
          <div class="text-caption">
            <strong>Formatos suportados:</strong>
            <span class="ml-1">
              <v-chip
                v-for="fmt in ACCEPTED_FORMATS"
                :key="fmt"
                class="mr-1 mt-1"
                label
                size="x-small"
                variant="outlined"
              >{{ fmt }}</v-chip>
            </span>
          </div>
        </v-alert>

        <!-- Drop zone -->
        <FileDropZone
          :accepted-extensions="ACCEPTED_EXTENSIONS"
          :disabled="uploading"
          :file="selectedFile"
          @change="onFileChange"
          @clear="clearFile"
        />

        <v-divider class="my-4" />

        <!-- Conexão -->
        <v-select
          v-model="form.connectionId"
          class="mb-3"
          clearable
          density="comfortable"
          hide-details="auto"
          :items="connections"
          item-title="name"
          item-value="id"
          label="Conexão de banco de dados"
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

        <!-- Nome do database -->
        <v-text-field
          v-model="form.databaseName"
          class="mb-4"
          density="comfortable"
          hide-details="auto"
          hint="Deixe em branco para inferir automaticamente pelo nome do arquivo"
          label="Nome do database"
          persistent-hint
          variant="outlined"
        />

        <v-divider class="mb-3" />

        <!-- Verificação de integridade -->
        <v-checkbox
          v-model="form.verifyIntegrity"
          color="primary"
          density="compact"
          hide-details
          label="Verificar integridade do arquivo antes de importar"
        >
          <template #label>
            <div>
              <span>Verificar integridade do arquivo</span>
              <div class="text-caption text-medium-emphasis">
                Valida magic bytes e estrutura básica do arquivo (recomendado)
              </div>
            </div>
          </template>
        </v-checkbox>
      </v-card-text>

      <!-- ── Etapa 2: Progresso / Resultado ── -->
      <v-card-text v-else-if="step === 'result'" class="pa-4">
        <!-- Sucesso -->
        <template v-if="importResult">
          <v-alert
            class="mb-4"
            color="success"
            icon="mdi-check-circle-outline"
            title="Backup importado com sucesso!"
            variant="tonal"
          />

          <!-- Avisos de integridade -->
          <v-alert
            v-if="importResult.integrity?.warnings?.length"
            class="mb-4"
            color="warning"
            density="compact"
            icon="mdi-alert-outline"
            variant="tonal"
          >
            <div v-for="warning in importResult.integrity.warnings" :key="warning" class="text-caption">
              {{ warning }}
            </div>
          </v-alert>

          <!-- Detalhes do backup importado -->
          <v-card variant="outlined">
            <v-card-text class="pa-3">
              <v-row dense>
                <v-col cols="6">
                  <div class="text-caption text-disabled">Arquivo</div>
                  <div class="text-body-2 font-weight-medium" style="word-break: break-all">
                    {{ importResult.backup.fileName ?? selectedFile?.name }}
                  </div>
                </v-col>

                <v-col cols="6">
                  <div class="text-caption text-disabled">Formato</div>
                  <v-chip color="primary" label size="x-small">{{ importResult.format.toUpperCase() }}</v-chip>
                </v-col>

                <v-col cols="6" class="mt-2">
                  <div class="text-caption text-disabled">Tamanho</div>
                  <div class="text-body-2">{{ formatFileSize(importResult.fileSize) }}</div>
                </v-col>

                <v-col cols="6" class="mt-2">
                  <div class="text-caption text-disabled">Database</div>
                  <div class="text-body-2">{{ importResult.backup.databaseName }}</div>
                </v-col>

                <v-col v-if="importResult.integrity" cols="12" class="mt-2">
                  <div class="text-caption text-disabled">Integridade</div>
                  <div class="d-flex align-center gap-1">
                    <v-icon
                      :color="importResult.integrity.valid ? 'success' : 'error'"
                      :icon="importResult.integrity.valid ? 'mdi-check-circle-outline' : 'mdi-close-circle-outline'"
                      size="16"
                    />
                    <span class="text-body-2">{{ importResult.integrity.message }}</span>
                  </div>
                </v-col>

                <v-col cols="12" class="mt-2">
                  <div class="text-caption text-disabled">Checksum (SHA-256)</div>
                  <div class="font-mono text-caption" style="word-break: break-all; opacity: 0.85">
                    {{ importResult.checksum }}
                  </div>
                </v-col>
              </v-row>
            </v-card-text>
          </v-card>
        </template>

        <!-- Erro -->
        <template v-else-if="errorMessage">
          <v-alert
            class="mb-4"
            color="error"
            icon="mdi-alert-circle-outline"
            title="Falha na importação"
            variant="tonal"
          >
            <div class="text-body-2 mt-1">{{ errorMessage }}</div>
          </v-alert>

          <v-btn color="primary" prepend-icon="mdi-arrow-left" variant="tonal" @click="resetToUpload">
            Tentar novamente
          </v-btn>
        </template>
      </v-card-text>

      <v-divider />

      <v-card-actions class="pa-3">
        <v-btn :disabled="uploading" variant="text" @click="close">
          {{ step === 'result' && importResult ? 'Fechar' : 'Cancelar' }}
        </v-btn>

        <v-spacer />

        <v-btn
          v-if="step === 'upload'"
          color="primary"
          :disabled="!canSubmit"
          :loading="uploading"
          prepend-icon="mdi-upload"
          variant="flat"
          @click="submit"
        >
          Importar
        </v-btn>
      </v-card-actions>

      <!-- Overlay de progresso durante upload -->
      <v-overlay
        v-model="uploading"
        class="align-center justify-center"
        contained
        persistent
      >
        <div class="text-center">
          <v-progress-circular class="mb-3" color="primary" indeterminate size="48" />
          <div class="text-body-2">Importando arquivo...</div>
          <div class="text-caption text-medium-emphasis">Aguarde, isso pode levar alguns segundos</div>
        </div>
      </v-overlay>
    </v-card>
  </v-dialog>
</template>

<script lang="ts" setup>
import { computed, reactive, ref } from 'vue'
import type { Connection, ImportBackupResult } from '@/types/api'
import { backupsApi } from '@/services/api'
import { useNotifier } from '@/composables/useNotifier'
import { getDatabaseColor } from '@/ui/database'
import { formatFileSize } from '@/utils/format'
import FileDropZone from './FileDropZone.vue'

// ─── Props / Emits ────────────────────────────────────────────────────────────

const emit = defineEmits<{
  (e: 'success'): void
}>()

// ─── Constantes ──────────────────────────────────────────────────────────────

const ACCEPTED_FORMATS = ['.sql', '.sql.gz', '.gz', '.dump', '.pgdump', '.zip', '.tar', '.tar.gz', '.tgz']
const ACCEPTED_EXTENSIONS = ACCEPTED_FORMATS.join(',')

// ─── Estado ──────────────────────────────────────────────────────────────────

const notify = useNotifier()

const isOpen = ref(false)
const uploading = ref(false)
const step = ref<'upload' | 'result'>('upload')
const importResult = ref<ImportBackupResult | null>(null)
const errorMessage = ref<string | null>(null)
const selectedFile = ref<File | null>(null)
const connections = ref<Connection[]>([])

const form = reactive({
  connectionId: null as number | null,
  databaseName: '',
  verifyIntegrity: true,
})

// ─── Computed ─────────────────────────────────────────────────────────────────

const canSubmit = computed(
  () => selectedFile.value !== null
)

// ─── API: ciclo de vida do dialog ─────────────────────────────────────────────

function open(availableConnections: Connection[]) {
  connections.value = availableConnections
  reset()
  isOpen.value = true
}

function close() {
  if (uploading.value) return
  isOpen.value = false
  if (importResult.value) {
    emit('success')
  }
}

function reset() {
  step.value = 'upload'
  importResult.value = null
  errorMessage.value = null
  selectedFile.value = null
  form.connectionId = null
  form.databaseName = ''
  form.verifyIntegrity = true
}

function resetToUpload() {
  step.value = 'upload'
  importResult.value = null
  errorMessage.value = null
}

// ─── Handlers de arquivo ──────────────────────────────────────────────────────

function onFileChange(file: File) {
  selectedFile.value = file

  // Pré-popular nome do database a partir do nome do arquivo
  if (!form.databaseName) {
    form.databaseName = inferDatabaseName(file.name)
  }
}

function clearFile() {
  selectedFile.value = null
}

/**
 * Tenta inferir o nome do database a partir do nome do arquivo.
 * Remove extensões conhecidas de formato de backup.
 */
function inferDatabaseName(fileName: string): string {
  return fileName
    .replace(/\.(sql\.gz|tar\.gz|tgz|sql|gz|dump|pgdump|pg_dump|zip|tar)$/i, '')
    .replace(/[_-]?\d{8,}[_-]?/g, '') // remove timestamps numéricos
    .replace(/[_-]+(backup|dump|import)$/i, '') // remove sufixos comuns
    .replace(/[^a-zA-Z0-9_]/g, '_')
    .replace(/^_+|_+$/g, '')
    .substring(0, 64) || ''
}

// ─── Submit ───────────────────────────────────────────────────────────────────

async function submit() {
  if (!canSubmit.value || !selectedFile.value) return

  uploading.value = true

  try {
    const formData = new FormData()
    formData.append('file', selectedFile.value)
    if (form.connectionId !== null) {
      formData.append('connectionId', String(form.connectionId))
    }
    if (form.databaseName.trim()) {
      formData.append('databaseName', form.databaseName.trim())
    }
    formData.append('verifyIntegrity', String(form.verifyIntegrity))

    const response = await backupsApi.import(formData)

    importResult.value = response.data ?? null
    step.value = 'result'

    notify('Backup importado com sucesso', 'success')
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Erro ao importar backup'
    errorMessage.value = message
    step.value = 'result'
    notify(message, 'error')
  } finally {
    uploading.value = false
  }
}

// ─── Expor para uso por ref ────────────────────────────────────────────────────

defineExpose({ open })
</script>
