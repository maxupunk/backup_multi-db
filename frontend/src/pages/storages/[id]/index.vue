<template>
  <div>
    <!-- Header -->
    <div class="d-flex align-center mb-6">
      <v-btn class="mr-3" icon="mdi-arrow-left" variant="text" to="/storages" />
      <div class="flex-grow-1">
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">
          {{ storage?.name ?? 'Armazenamento' }}
        </h1>
        <div v-if="storage" class="d-flex align-center ga-2">
          <StorageProviderIcon :provider="storage.provider" size="small" />
          <span class="text-body-2 text-medium-emphasis">{{ getProviderLabel(storage.provider) }}</span>
          <v-chip :color="storage.status === 'active' ? 'success' : 'grey'" label size="x-small">
            {{ storage.status === 'active' ? 'Ativo' : 'Inativo' }}
          </v-chip>
          <v-chip v-if="storage.isDefault" color="primary" label size="x-small" variant="tonal">
            Padrão
          </v-chip>
        </div>
      </div>
      <div v-if="storage" class="d-flex ga-2">
        <v-btn color="primary" prepend-icon="mdi-folder-search" variant="tonal" :to="`/storages/${id}/explore`">
          Explorar
        </v-btn>
        <v-menu location="bottom end">
          <template #activator="{ props }">
            <v-btn v-bind="props" icon="mdi-dots-vertical" variant="text" />
          </template>
          <v-list density="compact">
            <v-list-item prepend-icon="mdi-content-copy" title="Copiar para..." :to="`/storages/${id}/copy`" />
            <v-list-item prepend-icon="mdi-archive-arrow-down" title="Download" :to="`/storages/${id}/download`" />
          </v-list>
        </v-menu>
      </div>
    </div>

    <!-- Loading -->
    <v-progress-linear v-if="loadingStorage" color="primary" indeterminate />

    <template v-else-if="storage">
      <v-row>
        <!-- Edit form -->
        <v-col cols="12" md="8">
          <v-card class="mb-4">
            <v-card-title class="d-flex align-center">
              <v-icon class="mr-2" color="primary" icon="mdi-pencil" />
              Editar Armazenamento
            </v-card-title>

            <v-card-text>
              <v-form ref="formRef">
                <v-row>
                  <v-col cols="12" sm="8">
                    <v-text-field
                      v-model="form.name"
                      label="Nome *"
                      prepend-inner-icon="mdi-label"
                      :rules="[rules.required]"
                    />
                  </v-col>
                  <v-col cols="12" sm="4">
                    <v-select
                      v-model="form.status"
                      :items="statusOptions"
                      label="Status"
                      prepend-inner-icon="mdi-checkbox-marked-circle-outline"
                    />
                  </v-col>
                </v-row>

                <v-switch
                  v-model="form.isDefault"
                  class="mb-4"
                  color="primary"
                  hide-details
                  label="Padrão para backups"
                />

                <v-divider class="my-4" />

                <StorageFormFields
                  v-if="storage.provider"
                  :config="configForm"
                  is-edit-mode
                  :provider="storage.provider"
                  @update:config="configForm = $event"
                />
              </v-form>
            </v-card-text>

            <v-card-actions>
              <v-spacer />
              <v-btn color="primary" :loading="saving" prepend-icon="mdi-check" variant="flat" @click="save">
                Salvar Alterações
              </v-btn>
            </v-card-actions>
          </v-card>
        </v-col>

        <!-- Sidebar -->
        <v-col cols="12" md="4">
          <v-card class="mb-4">
            <v-card-title class="d-flex align-center">
              <v-icon class="mr-2" color="info" icon="mdi-connection" />
              Conectividade
            </v-card-title>
            <v-card-text>
              <StorageTestButton :storage-id="storage.id" />
            </v-card-text>
          </v-card>

          <v-card class="mb-4">
            <v-card-title class="d-flex align-center">
              <v-icon class="mr-2" color="warning" icon="mdi-information" />
              Informações
            </v-card-title>
            <v-card-text>
              <v-list density="compact">
                <v-list-item :subtitle="String(storage.id)" title="ID" />
                <v-list-item :subtitle="storage.type" title="Tipo" />
                <v-list-item :subtitle="getProviderLabel(storage.provider)" title="Provider" />
                <v-list-item :subtitle="formatDateTimePtBR(storage.createdAt)" title="Criado em" />
                <v-list-item :subtitle="formatDateTimePtBR(storage.updatedAt)" title="Atualizado em" />
              </v-list>
            </v-card-text>
          </v-card>

          <v-card class="mb-4" color="warning" variant="tonal">
            <v-card-title class="d-flex align-center">
              <v-icon class="mr-2" icon="mdi-power" />
              Status
            </v-card-title>
            <v-card-text>
              <v-btn
                v-if="storage.status === 'active'"
                block
                color="warning"
                :loading="deactivating"
                prepend-icon="mdi-power-off"
                variant="flat"
                @click="confirmDeactivate"
              >
                Desativar Armazenamento
              </v-btn>
              <v-btn
                v-else
                block
                color="success"
                :loading="activating"
                prepend-icon="mdi-power"
                variant="flat"
                @click="activateStorage"
              >
                Ativar Armazenamento
              </v-btn>
            </v-card-text>
          </v-card>

          <v-card color="error" variant="tonal">
            <v-card-title class="d-flex align-center">
              <v-icon class="mr-2" icon="mdi-alert" />
              Zona de Perigo
            </v-card-title>
            <v-card-text>
              <v-btn block color="error" prepend-icon="mdi-delete" variant="flat" @click="confirmDelete">
                Excluir Armazenamento
              </v-btn>
            </v-card-text>
          </v-card>
        </v-col>
      </v-row>
    </template>

    <!-- Not found -->
    <v-alert v-else type="error" variant="tonal">
      Armazenamento não encontrado.
    </v-alert>

    <!-- Deactivate dialog -->
    <v-dialog v-model="deactivateDialog" max-width="480">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon class="mr-2" color="warning" icon="mdi-power-off" />
          Desativar Armazenamento
        </v-card-title>
        <v-card-text>
          <template v-if="storage?.isDefault">
            <v-alert class="mb-4" type="warning" variant="tonal">
              Este é o armazenamento padrão. Selecione outro armazenamento ativo para assumir o papel de padrão.
            </v-alert>
            <v-select
              v-model="replacementDefaultId"
              :items="otherActiveStorages"
              item-title="name"
              item-value="id"
              label="Novo armazenamento padrão *"
              :loading="loadingReplacements"
              prepend-inner-icon="mdi-star"
            />
          </template>
          <template v-else>
            Tem certeza que deseja desativar <strong>{{ storage?.name }}</strong>? Conexões configuradas com este armazenamento não conseguirão criar novos backups.
          </template>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="deactivateDialog = false">Cancelar</v-btn>
          <v-btn
            color="warning"
            :disabled="deactivateDisabled"
            :loading="deactivating"
            variant="flat"
            @click="executeDeactivate"
          >
            Desativar
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- Delete dialog -->
    <v-dialog v-model="deleteDialog" max-width="480">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon class="mr-2" color="error" icon="mdi-alert" />
          Confirmar Exclusão
        </v-card-title>
        <v-card-text>
          <p class="mb-3">
            Tem certeza que deseja excluir <strong>{{ storage?.name }}</strong> do sistema?
          </p>
          <v-alert density="compact" icon="mdi-information" type="info" variant="tonal">
            Esta ação remove apenas o acesso configurado neste sistema. Os arquivos e dados
            armazenados no destino <strong>não serão excluídos</strong>.
          </v-alert>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="deleteDialog = false">Cancelar</v-btn>
          <v-btn color="error" :loading="deleting" variant="flat" @click="deleteStorage">Excluir</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script lang="ts" setup>
import type { Storage } from '@/types/api'
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useDisplay } from 'vuetify'
import { ApiError, storagesApi } from '@/services/api'
import { useStoragesStore } from '@/stores/storages'
import { useNotifier } from '@/composables/useNotifier'
import { getProviderLabel, getTypeForProvider } from '@/ui/storage'
import { formatDateTimePtBR } from '@/utils/format'
import StorageFormFields from '@/components/storages/StorageFormFields.vue'
import StorageProviderIcon from '@/components/storages/StorageProviderIcon.vue'
import StorageTestButton from '@/components/storages/StorageTestButton.vue'

const route = useRoute()
const router = useRouter()
const { mdAndUp } = useDisplay()
const storagesStore = useStoragesStore()
const notify = useNotifier()

const id = Number((route.params as { id: string }).id)

const storage = ref<Storage | null>(null)
const loadingStorage = ref(false)
const saving = ref(false)
const deleteDialog = ref(false)
const deleting = ref(false)
const deactivateDialog = ref(false)
const deactivating = ref(false)
const activating = ref(false)
const loadingReplacements = ref(false)
const replacementDefaultId = ref<number | null>(null)
const otherActiveStorages = ref<Storage[]>([])
const formRef = ref()

const deactivateDisabled = computed<boolean>(
  () => !!storage.value?.isDefault && replacementDefaultId.value === null,
)

const form = ref({
  name: '',
  status: 'active' as 'active' | 'inactive',
  isDefault: false,
})

const configForm = ref<Record<string, unknown>>({})

const statusOptions = [
  { title: 'Ativo', value: 'active' },
  { title: 'Inativo', value: 'inactive' },
]

const rules = {
  required: (v: unknown) => {
    const value = typeof v === 'string' ? v.trim() : v
    return !!value || 'Campo obrigatório'
  },
}

async function loadStorage () {
  loadingStorage.value = true
  try {
    const response = await storagesApi.get(id)
    storage.value = response.data ?? null

    if (storage.value) {
      form.value.name = storage.value.name
      form.value.status = storage.value.status
      form.value.isDefault = storage.value.isDefault

      const cfg = storage.value.config ?? {}
      configForm.value = { ...cfg }

      // Clear masked secrets
      clearMaskedSecrets()
    }
  } catch {
    notify('Erro ao carregar armazenamento', 'error')
  } finally {
    loadingStorage.value = false
  }
}

function clearMaskedSecrets () {
  if (!storage.value) return
  const s = storage.value

  if (['aws_s3', 'minio', 'cloudflare_r2'].includes(s.provider)) {
    if (configForm.value.secretAccessKey === '***') configForm.value.secretAccessKey = ''
  }
  if (s.provider === 'azure_blob') {
    if (configForm.value.connectionString === '***') configForm.value.connectionString = ''
  }
  if (s.provider === 'google_gcs') {
    if (configForm.value.credentialsJson === '***') configForm.value.credentialsJson = ''
  }
  if (s.provider === 'sftp') {
    if (configForm.value.password === '***') configForm.value.password = ''
    if (configForm.value.privateKey === '***') configForm.value.privateKey = ''
    if (configForm.value.passphrase === '***') configForm.value.passphrase = ''
  }
}

async function save () {
  const validation = await formRef.value?.validate?.()
  if (validation && 'valid' in validation && validation.valid === false) return

  if (!storage.value) return
  saving.value = true

  try {
    const payload: Record<string, unknown> = {}

    if (form.value.name.trim() !== storage.value.name) {
      payload.name = form.value.name.trim()
    }
    if (form.value.status !== storage.value.status) {
      payload.status = form.value.status
    }
    if (form.value.isDefault !== storage.value.isDefault) {
      payload.isDefault = form.value.isDefault
    }

    // Always send config + type + provider when config may have changed
    payload.type = getTypeForProvider(storage.value.provider)
    payload.provider = storage.value.provider
    payload.config = configForm.value

    await storagesStore.update(id, payload)
    notify('Armazenamento atualizado com sucesso', 'success')
    loadStorage()
  } catch (error) {
    const msg = error instanceof ApiError ? error.message : 'Erro ao atualizar'
    notify(msg, 'error')
  } finally {
    saving.value = false
  }
}

async function confirmDeactivate () {
  if (!storage.value) return

  if (storage.value.isDefault) {
    loadingReplacements.value = true
    try {
      const response = await storagesApi.list({ status: 'active', limit: 100 })
      otherActiveStorages.value = (response.data?.data ?? []).filter((s) => s.id !== id)
    } catch {
      notify('Erro ao carregar armazenamentos disponíveis', 'error')
      return
    } finally {
      loadingReplacements.value = false
    }
  }

  replacementDefaultId.value = null
  deactivateDialog.value = true
}

async function executeDeactivate () {
  if (!storage.value) return
  deactivating.value = true

  try {
    if (storage.value.isDefault && replacementDefaultId.value) {
      await storagesStore.update(replacementDefaultId.value, { isDefault: true })
    }
    await storagesStore.update(id, { status: 'inactive', isDefault: false })
    notify('Armazenamento desativado com sucesso', 'success')
    deactivateDialog.value = false
    loadStorage()
  } catch (error) {
    const msg = error instanceof ApiError ? error.message : 'Erro ao desativar armazenamento'
    notify(msg, 'error')
  } finally {
    deactivating.value = false
  }
}

async function activateStorage () {
  if (!storage.value) return
  activating.value = true

  try {
    await storagesStore.update(id, { status: 'active' })
    notify('Armazenamento ativado com sucesso', 'success')
    loadStorage()
  } catch (error) {
    const msg = error instanceof ApiError ? error.message : 'Erro ao ativar armazenamento'
    notify(msg, 'error')
  } finally {
    activating.value = false
  }
}

function confirmDelete () {
  deleteDialog.value = true
}

async function deleteStorage () {
  deleting.value = true
  try {
    await storagesStore.remove(id)
    notify('Armazenamento removido com sucesso', 'success')
    router.push('/storages')
  } catch (error) {
    const msg = error instanceof ApiError ? error.message : 'Erro ao remover armazenamento'
    notify(msg, 'error')
  } finally {
    deleting.value = false
  }
}

onMounted(loadStorage)
</script>
