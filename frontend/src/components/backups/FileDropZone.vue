<template>
  <div
    class="file-drop-zone rounded-lg pa-4"
    :class="{
      'file-drop-zone--dragging': isDragging,
      'file-drop-zone--has-file': !!file,
      'file-drop-zone--disabled': disabled,
    }"
    role="button"
    tabindex="0"
    @click="!disabled && !file && triggerFileInput()"
    @dragenter.prevent="onDragEnter"
    @dragleave.prevent="onDragLeave"
    @dragover.prevent
    @drop.prevent="onDrop"
    @keydown.enter="!disabled && !file && triggerFileInput()"
    @keydown.space.prevent="!disabled && !file && triggerFileInput()"
  >
    <!-- Input oculto -->
    <input
      ref="fileInputRef"
      :accept="acceptedExtensions"
      class="d-none"
      :disabled="disabled"
      type="file"
      @change="onInputChange"
    />

    <!-- Sem arquivo selecionado -->
    <div v-if="!file" class="text-center py-4">
      <v-icon
        :color="isDragging ? 'primary' : 'medium-emphasis'"
        :icon="isDragging ? 'mdi-cloud-download-outline' : 'mdi-upload-outline'"
        size="48"
        class="mb-3"
      />
      <div class="text-body-1 font-weight-medium mb-1">
        {{ isDragging ? 'Solte o arquivo aqui' : 'Arraste e solte ou clique para selecionar' }}
      </div>
      <div class="text-caption text-medium-emphasis">
        Tamanho máximo: 500 MB
      </div>
    </div>

    <!-- Arquivo selecionado -->
    <div v-else class="d-flex align-center gap-3 px-2">
      <v-icon color="primary" :icon="fileIcon" size="36" />

      <div class="flex-grow-1 overflow-hidden">
        <div class="text-body-2 font-weight-medium text-truncate">{{ file.name }}</div>
        <div class="text-caption text-medium-emphasis">
          {{ formatFileSize(file.size) }} &bull; {{ fileExtension.toUpperCase() }}
        </div>
      </div>

      <v-btn
        v-if="!disabled"
        color="error"
        density="compact"
        icon="mdi-close-circle-outline"
        variant="text"
        @click.stop="$emit('clear')"
      />
    </div>
  </div>

  <!-- Erro de validação -->
  <div v-if="validationError" class="text-caption text-error mt-1 ml-1">
    {{ validationError }}
  </div>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { formatFileSize } from '@/utils/format'

// ─── Props / Emits ────────────────────────────────────────────────────────────

const props = defineProps<{
  file: File | null
  acceptedExtensions: string
  disabled?: boolean
}>()

const emit = defineEmits<{
  (e: 'change', file: File): void
  (e: 'clear'): void
}>()

// ─── Estado ──────────────────────────────────────────────────────────────────

const fileInputRef = ref<HTMLInputElement | null>(null)
const isDragging = ref(false)
const validationError = ref<string | null>(null)
let dragCounter = 0

// ─── Computed ─────────────────────────────────────────────────────────────────

const fileExtension = computed(() => {
  if (!props.file) return ''
  const name = props.file.name.toLowerCase()
  if (name.endsWith('.sql.gz')) return 'sql.gz'
  if (name.endsWith('.tar.gz')) return 'tar.gz'
  return name.split('.').pop() ?? ''
})

const fileIcon = computed(() => {
  const ext = fileExtension.value
  if (ext === 'zip') return 'mdi-folder-zip-outline'
  if (ext === 'tar' || ext === 'tar.gz' || ext === 'tgz') return 'mdi-archive-outline'
  if (ext === 'gz' || ext === 'sql.gz') return 'mdi-zip-box-outline'
  return 'mdi-database-outline'
})

// ─── Handlers ────────────────────────────────────────────────────────────────

function onDragEnter() {
  dragCounter++
  isDragging.value = true
}

function onDragLeave() {
  dragCounter--
  if (dragCounter <= 0) {
    dragCounter = 0
    isDragging.value = false
  }
}

function onDrop(event: DragEvent) {
  dragCounter = 0
  isDragging.value = false

  if (props.disabled || props.file) return

  const droppedFile = event.dataTransfer?.files[0]
  if (droppedFile) {
    validateAndEmit(droppedFile)
  }
}

function onInputChange(event: Event) {
  const input = event.target as HTMLInputElement
  const selectedFile = input.files?.[0]
  if (selectedFile) {
    validateAndEmit(selectedFile)
  }
  // Limpa o input para permitir re-selecionar o mesmo arquivo
  input.value = ''
}

function triggerFileInput() {
  fileInputRef.value?.click()
}

// ─── Validação ────────────────────────────────────────────────────────────────

const ACCEPTED_EXT_LIST = [
  '.sql', '.sql.gz', '.gz', '.dump', '.pgdump', '.pg_dump',
  '.zip', '.tar', '.tar.gz', '.tgz',
]

const MAX_SIZE_BYTES = 500 * 1024 * 1024 // 500 MB

function validateAndEmit(file: File): void {
  validationError.value = null

  const name = file.name.toLowerCase()
  const hasValidExt = ACCEPTED_EXT_LIST.some((ext) => name.endsWith(ext))

  if (!hasValidExt) {
    validationError.value =
      `Formato não suportado. Aceitos: ${ACCEPTED_EXT_LIST.join(', ')}`
    return
  }

  if (file.size > MAX_SIZE_BYTES) {
    validationError.value = `Arquivo muito grande. Tamanho máximo: 500 MB`
    return
  }

  emit('change', file)
}
</script>

<style scoped>
.file-drop-zone {
  border: 2px dashed rgba(var(--v-theme-on-surface), 0.2);
  cursor: pointer;
  transition: border-color 0.2s, background-color 0.2s;
  min-height: 120px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.file-drop-zone:hover:not(.file-drop-zone--disabled):not(.file-drop-zone--has-file) {
  border-color: rgb(var(--v-theme-primary));
  background-color: rgba(var(--v-theme-primary), 0.04);
}

.file-drop-zone:focus-visible {
  outline: 2px solid rgb(var(--v-theme-primary));
  outline-offset: 2px;
}

.file-drop-zone--dragging {
  border-color: rgb(var(--v-theme-primary));
  background-color: rgba(var(--v-theme-primary), 0.08);
}

.file-drop-zone--has-file {
  border-style: solid;
  border-color: rgba(var(--v-theme-on-surface), 0.15);
  cursor: default;
}

.file-drop-zone--disabled {
  cursor: not-allowed;
  opacity: 0.5;
}
</style>
