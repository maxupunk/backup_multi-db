<template>
  <div>
    <!-- Header -->
    <div class="d-flex align-center mb-6">
      <v-btn class="mr-4" icon="mdi-arrow-left" to="/connections" variant="text" />
      <div>
        <h1 class="text-h4 font-weight-bold mb-1">
          {{ isEditing ? 'Editar Conexão' : 'Nova Conexão' }}
        </h1>
        <p class="text-body-2 text-medium-emphasis">
          {{ isEditing ? 'Atualize os dados da conexão' : 'Configure uma nova conexão de banco de dados' }}
        </p>
      </div>
    </div>

    <v-row>
      <v-col cols="12" md="8">
        <v-card>
          <v-card-text>
            <v-form ref="formRef" :disabled="loading" @submit.prevent="submit">
              <!-- Basic Info -->
              <h3 class="text-h6 mb-4">Informações Básicas</h3>

              <v-row>
                <v-col cols="12" sm="8">
                  <v-text-field v-model="form.name" label="Nome da Conexão *" placeholder="Ex: Produção MySQL"
                    prepend-inner-icon="mdi-label" :rules="[rules.required]" />
                </v-col>

                <v-col cols="12" sm="4">
                  <v-select v-model="form.type" :items="databaseTypes" label="Tipo de Banco *"
                    prepend-inner-icon="mdi-database" :rules="[rules.required]" @update:model-value="onTypeChange" />
                </v-col>
              </v-row>

              <v-divider class="my-6" />

              <!-- Connection Details -->
              <h3 class="text-h6 mb-4">Dados de Conexão</h3>

              <v-row>
                <v-col cols="12" sm="8">
                  <v-text-field v-model="form.host" label="Host *" placeholder="localhost ou IP do servidor"
                    prepend-inner-icon="mdi-server" :rules="[rules.required]" />
                </v-col>

                <v-col cols="12" sm="4">
                  <v-text-field v-model.number="form.port" label="Porta *" prepend-inner-icon="mdi-ethernet"
                    :rules="[rules.required, rules.port]" type="number" />
                </v-col>

                <v-col cols="12" sm="6">
                  <v-text-field v-model="form.username" label="Usuário *" prepend-inner-icon="mdi-account"
                    :rules="[rules.required]" />
                </v-col>

                <v-col cols="12" sm="6">
                  <v-text-field v-model="form.password" :append-inner-icon="showPassword ? 'mdi-eye-off' : 'mdi-eye'"
                    :label="isEditing ? 'Nova Senha (deixe vazio para manter)' : 'Senha'" prepend-inner-icon="mdi-lock"
                    :rules="[]" :type="showPassword ? 'text' : 'password'"
                    @click:append-inner="showPassword = !showPassword" @update:model-value="markPasswordAsModified" />
                </v-col>

                <!-- Botão para testar conexão e listar databases -->
                <v-col cols="12">
                  <v-btn color="info" :disabled="!canDiscoverDatabases" :loading="discoveringDatabases"
                    prepend-icon="mdi-database-search" variant="tonal" @click="discoverDatabases">
                    Testar e Listar Bancos de Dados
                  </v-btn>

                  <v-alert v-if="discoverError" class="mt-3" closable color="error" density="compact" variant="tonal"
                    @click:close="discoverError = ''">
                    {{ discoverError }}
                  </v-alert>

                  <v-alert v-if="availableDatabases.length > 0" class="mt-3" color="success" density="compact"
                    variant="tonal">
                    <template #prepend>
                      <v-icon icon="mdi-check-circle" />
                    </template>
                    Conexão bem-sucedida! {{ availableDatabases.length }} banco(s) de dados encontrado(s).
                  </v-alert>
                </v-col>

                <!-- Seleção de databases (múltipla) -->
                <v-col cols="12">
                  <v-select v-model="form.databases" chips closable-chips :disabled="availableDatabases.length === 0"
                    :items="availableDatabases" label="Selecione os Bancos de Dados para Backup *" multiple
                    :no-data-text="availableDatabases.length === 0 ? 'Clique em Testar e Listar Bancos primeiro' : 'Nenhum banco de dados encontrado'"
                    prepend-inner-icon="mdi-database-multiple" :rules="[rules.requiredArray]">
                    <template #chip="{ item, props: chipProps }">
                      <v-chip v-bind="chipProps" color="primary" variant="outlined">
                        <v-icon class="mr-1" icon="mdi-database" size="small" />
                        {{ item.title }}
                      </v-chip>
                    </template>
                  </v-select>
                </v-col>
              </v-row>

              <v-divider class="my-6" />

              <h3 class="text-h6 mb-4">Armazenamento</h3>

              <v-row>
                <v-col cols="12">
                  <v-select v-model="form.storageDestinationId" clearable :items="storageDestinationOptions"
                    label="Destino de Armazenamento" prepend-inner-icon="mdi-folder-upload"
                    :loading="loadingDestinations" :disabled="loadingDestinations" />
                </v-col>
              </v-row>

              <v-divider class="my-6" />

              <!-- Scheduling -->
              <h3 class="text-h6 mb-4">Agendamento</h3>

              <v-row>
                <v-col cols="12" sm="6">
                  <v-switch v-model="form.scheduleEnabled" color="primary" hide-details
                    label="Habilitar backup automático" />
                </v-col>

                <v-col cols="12" sm="6">
                  <v-select v-model="form.scheduleFrequency" clearable :disabled="!form.scheduleEnabled"
                    :items="scheduleFrequencies" label="Frequência" prepend-inner-icon="mdi-clock-outline" />
                </v-col>
              </v-row>

              <v-divider class="my-6" />

              <!-- Actions -->
              <div class="actions-bar d-flex justify-end ga-3">
                <v-btn :block="!mdAndUp" to="/connections" variant="outlined">
                  Cancelar
                </v-btn>

                <v-btn v-if="isEditing" :block="!mdAndUp" color="info" :loading="testing" prepend-icon="mdi-connection"
                  variant="tonal" @click="testConnection">
                  Testar Conexão
                </v-btn>

                <v-btn :block="!mdAndUp" color="primary" :loading="saving"
                  :prepend-icon="isEditing ? 'mdi-content-save' : 'mdi-plus'" type="submit">
                  {{ isEditing ? 'Salvar Alterações' : 'Criar Conexão' }}
                </v-btn>
              </div>
            </v-form>
          </v-card-text>
        </v-card>
      </v-col>

      <!-- Sidebar Info -->
      <v-col cols="12" md="4">
        <v-card class="mb-4">
          <v-card-title class="d-flex align-center">
            <v-icon class="mr-2" color="info" icon="mdi-information" />
            Dicas
          </v-card-title>

          <v-card-text>
            <v-list class="tips-list" density="compact">
              <v-list-item>
                <template #prepend>
                  <v-icon color="success" icon="mdi-shield-check" size="20" />
                </template>
                <v-list-item-title class="text-body-2 tips-text">
                  Senhas são criptografadas com AES-256
                </v-list-item-title>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-icon color="info" icon="mdi-backup-restore" size="20" />
                </template>
                <v-list-item-title class="text-body-2 tips-text">
                  Use um usuário com permissão de leitura
                </v-list-item-title>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-icon color="warning" icon="mdi-clock" size="20" />
                </template>
                <v-list-item-title class="text-body-2 tips-text">
                  Backups são comprimidos com gzip
                </v-list-item-title>
              </v-list-item>
            </v-list>
          </v-card-text>
        </v-card>

        <!-- Port reference -->
        <v-card>
          <v-card-title class="d-flex align-center">
            <v-icon class="mr-2" icon="mdi-ethernet" />
            Portas Padrão
          </v-card-title>

          <v-card-text>
            <v-list density="compact">
              <v-list-item>
                <template #prepend>
                  <v-chip class="mr-2" color="orange" label size="small">MySQL</v-chip>
                </template>
                <v-list-item-title>3306</v-list-item-title>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-chip class="mr-2" color="teal" label size="small">MariaDB</v-chip>
                </template>
                <v-list-item-title>3306</v-list-item-title>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-chip class="mr-2" color="blue" label size="small">PostgreSQL</v-chip>
                </template>
                <v-list-item-title>5432</v-list-item-title>
              </v-list-item>
            </v-list>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>
  </div>
</template>

<script lang="ts" setup>
import type { DatabaseType, ScheduleFrequency, StorageDestination } from '@/types/api'
import { computed, onMounted, reactive, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useDisplay } from 'vuetify'
import { connectionsApi, storageDestinationsApi } from '@/services/api'
import { useNotifier } from '@/composables/useNotifier'
import { useStorageDestinationOptions } from '@/composables/useStorageDestinationOptions'
import { databaseTypeOptions, defaultDatabasePorts } from '@/ui/database'

const route = useRoute()
const router = useRouter()
const notify = useNotifier()
const { mdAndUp } = useDisplay()

const formRef = ref()
const showPassword = ref(false)
const saving = ref(false)
const testing = ref(false)
const loading = ref(false)
const passwordModified = ref(false)
const loadingDestinations = ref(false)
const storageDestinations = ref<StorageDestination[]>([])

// Estado para descoberta de databases
const discoveringDatabases = ref(false)
const availableDatabases = ref<string[]>([])
const discoverError = ref('')

const isEditing = computed(() => {
  const params = route.params as { id?: string }
  const id = params.id
  return !!id && id !== 'new' && !Number.isNaN(Number(id))
})

// Helper para obter o ID da conexão de forma segura
function getConnectionId(): number {
  const params = route.params as { id?: string }
  const id = Number(params.id)
  if (Number.isNaN(id)) {
    throw new Error('ID inválido')
  }
  return id
}

const form = reactive({
  name: '',
  type: 'mysql' as DatabaseType,
  host: 'localhost',
  port: 3306,
  databases: [] as string[],
  username: '',
  password: '',
  storageDestinationId: null as number | null,
  scheduleEnabled: false,
  scheduleFrequency: null as ScheduleFrequency | null,
})

const databaseTypes = databaseTypeOptions

const scheduleFrequencies = [
  { title: 'A cada 1 hora', value: '1h' },
  { title: 'A cada 6 horas', value: '6h' },
  { title: 'A cada 12 horas', value: '12h' },
  { title: 'A cada 24 horas', value: '24h' },
]

const defaultPorts = defaultDatabasePorts

const rules = {
  required: (v: string) => !!v || 'Campo obrigatório',
  requiredArray: (v: unknown[]) => (Array.isArray(v) && v.length > 0) || 'Selecione pelo menos um banco de dados',
  port: (v: number) => (v > 0 && v <= 65_535) || 'Porta inválida',
}

// Computed para verificar se pode testar conexão
const canDiscoverDatabases = computed(() => {
  return form.host && form.port && form.username && form.type
})

function onTypeChange(type: DatabaseType) {
  form.port = defaultPorts[type]
}

function markPasswordAsModified() {
  passwordModified.value = true
}

/**
 * Descobre os bancos de dados disponíveis com as credenciais atuais
 */
async function discoverDatabases() {
  discoveringDatabases.value = true
  discoverError.value = ''
  availableDatabases.value = []

  try {
    const response = await connectionsApi.discoverDatabases({
      type: form.type,
      host: form.host,
      port: form.port,
      username: form.username,
      password: form.password || undefined,
    })

    if (response.success && response.data?.databases) {
      availableDatabases.value = response.data.databases

      // Se estiver editando e tiver um database já selecionado, mantém a seleção
      // se o database existir na lista disponível
      if (isEditing.value && form.databases.length > 0) {
        form.databases = form.databases.filter(db => availableDatabases.value.includes(db))
      }
    }
  } catch (error) {
    discoverError.value = error instanceof Error ? error.message : 'Erro ao conectar'
    availableDatabases.value = []
  } finally {
    discoveringDatabases.value = false
  }
}

const storageDestinationOptions = useStorageDestinationOptions(storageDestinations)

async function loadStorageDestinations() {
  loadingDestinations.value = true
  try {
    const response = await storageDestinationsApi.list({ status: 'active', limit: 100 })
    storageDestinations.value = response.data?.data ?? []
  } catch {
    storageDestinations.value = []
  } finally {
    loadingDestinations.value = false
  }
}

async function loadConnection() {
  if (!isEditing.value) return

  loading.value = true
  try {
    const response = await connectionsApi.get(getConnectionId())
    const connection = response.data
    if (connection) {
      form.name = connection.name
      form.type = connection.type
      form.host = connection.host
      form.port = connection.port
      // Carrega os databases do array retornado pela API
      const dbNames = connection.databases?.map(db => db.databaseName) ?? []
      form.databases = dbNames
      // Adiciona os databases à lista de disponíveis para edição
      if (dbNames.length > 0) {
        availableDatabases.value = [...dbNames]
      }
      form.username = connection.username
      form.storageDestinationId = connection.storageDestinationId ?? null
      form.scheduleEnabled = connection.scheduleEnabled
      form.scheduleFrequency = connection.scheduleFrequency
    }
  } catch {
    notify('Erro ao carregar conexão', 'error')
    router.push('/connections')
  } finally {
    loading.value = false
  }
}

async function submit() {
  const { valid } = await formRef.value.validate()
  if (!valid) return

  saving.value = true
  try {
    if (isEditing.value) {
      const payload: Record<string, unknown> = {
        name: form.name,
        type: form.type,
        host: form.host,
        port: form.port,
        databases: form.databases,
        username: form.username,
        storageDestinationId: form.storageDestinationId,
        scheduleEnabled: form.scheduleEnabled,
        scheduleFrequency: form.scheduleFrequency,
      }

      // Enviar senha apenas se foi modificada explicitamente
      if (passwordModified.value) {
        payload.password = form.password
      }

      await connectionsApi.update(getConnectionId(), payload)
      notify('Conexão atualizada com sucesso', 'success')
    } else {
      // Cria uma única conexão com todos os databases selecionados
      await connectionsApi.create({
        name: form.name,
        type: form.type,
        host: form.host,
        port: form.port,
        databases: form.databases,
        username: form.username,
        password: form.password,
        storageDestinationId: form.storageDestinationId,
        scheduleEnabled: form.scheduleEnabled,
        scheduleFrequency: form.scheduleFrequency || undefined,
      })
      notify(
        form.databases.length > 1
          ? `Conexão criada com ${form.databases.length} databases`
          : 'Conexão criada com sucesso',
        'success',
      )
    }

    router.push('/connections')
  } catch {
    notify(
      isEditing.value ? 'Erro ao atualizar conexão' : 'Erro ao criar conexão',
      'error',
    )
  } finally {
    saving.value = false
  }
}

async function testConnection() {
  testing.value = true
  try {
    const response = await connectionsApi.test(getConnectionId())
    notify(
      `Conexão bem-sucedida! Latência: ${response.data?.latencyMs}ms`,
      'success',
    )
  } catch {
    notify('Falha ao testar conexão', 'error')
  } finally {
    testing.value = false
  }
}

onMounted(() => {
  loadStorageDestinations()
  loadConnection()
})
</script>

<style scoped>
.actions-bar {
  flex-direction: column;
}

@media (min-width: 960px) {
  .actions-bar {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
  }
}

.tips-list :deep(.v-list-item) {
  align-items: flex-start;
}

.tips-text {
  white-space: normal;
  overflow: visible;
  text-overflow: unset;
}
</style>
