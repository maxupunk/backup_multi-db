<template>
  <div>
    <!-- Header -->
    <div class="d-flex align-center mb-6">
      <v-btn class="mr-3" icon="mdi-arrow-left" variant="text" to="/storages" />
      <div>
        <h1 class="font-weight-bold mb-1" :class="mdAndUp ? 'text-h4' : 'text-h5'">Novo Armazenamento</h1>
        <p class="text-body-2 text-medium-emphasis">
          Configure um novo destino de armazenamento
        </p>
      </div>
    </div>

    <v-stepper v-model="step" :mobile="!mdAndUp">
      <v-stepper-header>
        <v-stepper-item :complete="step > 1" title="Provider" :value="1" />
        <v-divider />
        <v-stepper-item :complete="step > 2" title="Configuração" :value="2" />
        <v-divider />
        <v-stepper-item title="Revisão" :value="3" />
      </v-stepper-header>

      <v-stepper-window>
        <!-- Step 1: Provider -->
        <v-stepper-window-item :value="1">
          <v-card flat>
            <v-card-text>
              <p class="text-body-1 mb-4">Selecione o tipo de armazenamento:</p>
              <v-row>
                <v-col
                  v-for="provider in storageProviderOptions"
                  :key="provider.value"
                  cols="6"
                  md="3"
                  sm="4"
                >
                  <v-card
                    :class="{ 'border-primary': form.provider === provider.value }"
                    :color="form.provider === provider.value ? 'primary' : undefined"
                    hover
                    :variant="form.provider === provider.value ? 'tonal' : 'outlined'"
                    @click="selectProvider(provider.value as StorageProvider)"
                  >
                    <v-card-text class="text-center pa-4">
                      <v-icon class="mb-2" :icon="provider.icon" size="36" />
                      <div class="text-body-2 font-weight-medium">{{ provider.title }}</div>
                    </v-card-text>
                  </v-card>
                </v-col>
              </v-row>
            </v-card-text>

            <v-card-actions>
              <v-spacer />
              <v-btn color="primary" :disabled="!form.provider" variant="flat" @click="step = 2">
                Próximo
              </v-btn>
            </v-card-actions>
          </v-card>
        </v-stepper-window-item>

        <!-- Step 2: Config -->
        <v-stepper-window-item :value="2">
          <v-card flat>
            <v-card-text>
              <v-form ref="formRef">
                <v-row class="mb-4">
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
                  label="Definir como padrão para backups"
                />

                <v-divider class="mb-6" />

                <StorageFormFields
                  v-if="form.provider"
                  :config="configForm"
                  :provider="form.provider"
                  @update:config="configForm = $event"
                />
              </v-form>
            </v-card-text>

            <v-card-actions>
              <v-btn variant="text" @click="step = 1">Voltar</v-btn>
              <v-spacer />
              <v-btn color="primary" variant="flat" @click="goToReview">
                Próximo
              </v-btn>
            </v-card-actions>
          </v-card>
        </v-stepper-window-item>

        <!-- Step 3: Review -->
        <v-stepper-window-item :value="3">
          <v-card flat>
            <v-card-text>
              <v-alert class="mb-4" density="compact" type="info" variant="tonal">
                Revise as informações e teste a conexão antes de salvar.
              </v-alert>

              <v-list density="compact" lines="two">
                <v-list-item prepend-icon="mdi-label" :subtitle="form.name" title="Nome" />
                <v-list-item prepend-icon="mdi-shape" :subtitle="providerLabel" title="Provider" />
                <v-list-item prepend-icon="mdi-checkbox-marked-circle-outline" :subtitle="form.status === 'active' ? 'Ativo' : 'Inativo'" title="Status" />
                <v-list-item v-if="form.isDefault" prepend-icon="mdi-star" subtitle="Sim" title="Padrão para backups" />
              </v-list>

              <v-divider class="my-4" />

              <div class="d-flex align-center ga-3">
                <v-btn
                  :color="testStatus === 'success' ? 'success' : testStatus === 'error' ? 'error' : 'primary'"
                  :loading="testing"
                  :prepend-icon="testStatusIcon"
                  variant="tonal"
                  @click="testBeforeSave"
                >
                  {{ testStatusLabel }}
                </v-btn>

                <span v-if="testStatus === 'error'" class="text-error text-caption">
                  {{ testError }}
                </span>
              </div>
            </v-card-text>

            <v-card-actions>
              <v-btn variant="text" @click="step = 2">Voltar</v-btn>
              <v-spacer />
              <v-btn
                color="primary"
                :disabled="testStatus !== 'success'"
                :loading="saving"
                prepend-icon="mdi-check"
                variant="flat"
                @click="save"
              >
                Salvar
              </v-btn>
            </v-card-actions>
          </v-card>
        </v-stepper-window-item>
      </v-stepper-window>
    </v-stepper>
  </div>
</template>

<script lang="ts" setup>
import type { StorageProvider } from '@/types/api'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useDisplay } from 'vuetify'
import { ApiError, storagesApi } from '@/services/api'
import { useStoragesStore } from '@/stores/storages'
import { useNotifier } from '@/composables/useNotifier'
import { getProviderLabel, getTypeForProvider, storageProviderOptions } from '@/ui/storage'
import StorageFormFields from '@/components/storages/StorageFormFields.vue'

const router = useRouter()
const { mdAndUp } = useDisplay()
const storagesStore = useStoragesStore()
const notify = useNotifier()

const step = ref(1)
const formRef = ref()
const saving = ref(false)
const testing = ref(false)
const testStatus = ref<'idle' | 'success' | 'error'>('idle')
const testError = ref('')
const tempStorageId = ref<number | null>(null)

const form = ref({
  name: '',
  provider: null as StorageProvider | null,
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

const providerLabel = computed(() =>
  form.value.provider ? getProviderLabel(form.value.provider) : '-',
)

const testStatusIcon = computed(() => {
  if (testStatus.value === 'success') return 'mdi-check-circle'
  if (testStatus.value === 'error') return 'mdi-alert-circle'
  return 'mdi-connection'
})

const testStatusLabel = computed(() => {
  if (testing.value) return 'Testando...'
  if (testStatus.value === 'success') return 'Conexão OK'
  if (testStatus.value === 'error') return 'Falhou'
  return 'Testar Conexão'
})

function selectProvider (provider: StorageProvider) {
  form.value.provider = provider
  configForm.value = provider === 'local' ? { basePath: '/app_data/backups' } : {}
  testStatus.value = 'idle'
}

async function goToReview () {
  const validation = await formRef.value?.validate?.()
  if (validation && 'valid' in validation && validation.valid === false) return
  step.value = 3
}

async function testBeforeSave () {
  if (!form.value.provider) return
  testing.value = true
  testStatus.value = 'idle'
  testError.value = ''

  try {
    // Create temporarily to test, then we'll use that ID to save
    const created = await storagesStore.create({
      name: form.value.name.trim(),
      type: getTypeForProvider(form.value.provider),
      provider: form.value.provider,
      status: form.value.status,
      isDefault: form.value.isDefault,
      config: configForm.value,
    })
    tempStorageId.value = created.id

    await storagesApi.test(created.id)
    testStatus.value = 'success'
  } catch (error) {
    testStatus.value = 'error'
    testError.value = error instanceof ApiError ? error.message : 'Erro ao testar conexão'

    // Clean up the temporary storage if test failed
    if (tempStorageId.value) {
      try { await storagesStore.remove(tempStorageId.value) } catch { /* ignore */ }
      tempStorageId.value = null
    }
  } finally {
    testing.value = false
  }
}

async function save () {
  if (tempStorageId.value) {
    notify('Armazenamento criado com sucesso', 'success')
    router.push(`/storages/${tempStorageId.value}`)
    return
  }

  if (!form.value.provider) return

  saving.value = true
  try {
    const created = await storagesStore.create({
      name: form.value.name.trim(),
      type: getTypeForProvider(form.value.provider),
      provider: form.value.provider,
      status: form.value.status,
      isDefault: form.value.isDefault,
      config: configForm.value,
    })

    notify('Armazenamento criado com sucesso', 'success')
    router.push(`/storages/${created.id}`)
  } catch (error) {
    const msg = error instanceof ApiError ? error.message : 'Erro ao criar armazenamento'
    notify(msg, 'error')
  } finally {
    saving.value = false
  }
}
</script>
