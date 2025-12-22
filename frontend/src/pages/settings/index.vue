<template>
  <div>
    <!-- Header -->
    <div class="mb-6">
      <h1 class="text-h4 font-weight-bold mb-1">Configurações</h1>
      <p class="text-body-2 text-medium-emphasis">
        Configurações gerais do sistema
      </p>
    </div>

    <v-row>
      <v-col cols="12" md="6">
        <!-- Appearance -->
        <v-card class="mb-4">
          <v-card-title class="d-flex align-center">
            <v-icon class="mr-2" color="secondary" icon="mdi-palette" />
            Aparência
          </v-card-title>

          <v-card-text>
            <v-list>
              <v-list-item>
                <v-list-item-title>Tema</v-list-item-title>
                <v-list-item-subtitle>
                  Escolha entre tema claro ou escuro
                </v-list-item-subtitle>

                <template #append>
                  <v-btn-toggle
                    v-model="settings.theme"
                    density="compact"
                    mandatory
                    rounded="lg"
                    @update:model-value="applyTheme"
                  >
                    <v-btn icon="mdi-weather-sunny" value="light" />
                    <v-btn icon="mdi-weather-night" value="dark" />
                    <v-btn icon="mdi-desktop-tower-monitor" value="system" />
                  </v-btn-toggle>
                </template>
              </v-list-item>
            </v-list>
          </v-card-text>
        </v-card>

        <!-- System Info -->
        <v-card>
          <v-card-title class="d-flex align-center">
            <v-icon class="mr-2" color="info" icon="mdi-information" />
            Informações do Sistema
          </v-card-title>

          <v-card-text>
            <v-list density="compact">
              <v-list-item>
                <template #prepend>
                  <v-icon icon="mdi-tag" />
                </template>
                <v-list-item-title>Versão</v-list-item-title>
                <template #append>
                  <span class="text-medium-emphasis">1.0.0</span>
                </template>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-icon icon="mdi-server" />
                </template>
                <v-list-item-title>Status da API</v-list-item-title>
                <template #append>
                  <v-chip :color="apiStatus === 'online' ? 'success' : 'error'" label size="small">
                    {{ apiStatus === 'online' ? 'Online' : 'Offline' }}
                  </v-chip>
                </template>
              </v-list-item>

              <v-list-item v-if="apiLatency">
                <template #prepend>
                  <v-icon icon="mdi-speedometer" />
                </template>
                <v-list-item-title>Latência</v-list-item-title>
                <template #append>
                  <span class="text-medium-emphasis">{{ apiLatency }}ms</span>
                </template>
              </v-list-item>
            </v-list>

            <v-btn
              block
              class="mt-4"
              color="info"
              :loading="checkingApi"
              prepend-icon="mdi-refresh"
              variant="tonal"
              @click="checkApi"
            >
              Verificar Conexão
            </v-btn>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="6">
        <v-card class="mb-4">
          <v-card-title class="d-flex align-center justify-space-between">
            <div class="d-flex align-center">
              <v-icon class="mr-2" color="primary" icon="mdi-folder-upload" />
              Destinos de Armazenamento
            </div>

            <v-btn color="primary" prepend-icon="mdi-plus" variant="tonal" @click="openCreateDestination">
              Novo
            </v-btn>
          </v-card-title>

          <v-card-text>
            <v-text-field
              v-model="destinationFilters.search"
              class="mb-4"
              clearable
              density="comfortable"
              hide-details
              label="Buscar destino"
              prepend-inner-icon="mdi-magnify"
              variant="outlined"
              @update:model-value="debouncedLoadDestinations"
            />

            <v-data-table
              class="elevation-0"
              :headers="destinationHeaders"
              :items="storageDestinations"
              :items-per-page="5"
              :loading="loadingDestinations"
              :mobile="!mdAndUp"
            >
              <template #item.type="{ item }">
                <v-chip label size="small" variant="tonal">
                  {{ formatDestinationType(item.type) }}
                </v-chip>
              </template>

              <template #item.status="{ item }">
                <v-chip :color="item.status === 'active' ? 'success' : 'grey'" label size="small">
                  {{ item.status === 'active' ? 'Ativo' : 'Inativo' }}
                </v-chip>
              </template>

              <template #item.isDefault="{ item }">
                <v-chip v-if="item.isDefault" color="primary" label size="small" variant="tonal">
                  Padrão
                </v-chip>
              </template>

              <template #item.actions="{ item }">
                <template v-if="mdAndUp">
                  <v-btn icon="mdi-pencil" size="small" variant="text" @click="openEditDestination(item.id)" />
                  <v-btn
                    color="error"
                    icon="mdi-delete"
                    size="small"
                    variant="text"
                    @click="confirmDeleteDestination(item)"
                  />
                </template>

                <v-menu v-else location="bottom end">
                  <template #activator="{ props }">
                    <v-btn v-bind="props" icon="mdi-dots-vertical" variant="text" />
                  </template>
                  <v-list density="compact">
                    <v-list-item prepend-icon="mdi-pencil" title="Editar" @click="openEditDestination(item.id)" />
                    <v-list-item
                      prepend-icon="mdi-delete"
                      title="Excluir"
                      @click="confirmDeleteDestination(item)"
                    />
                  </v-list>
                </v-menu>
              </template>

              <template #no-data>
                <div class="text-center py-6 text-medium-emphasis">
                  Nenhum destino encontrado
                </div>
              </template>
            </v-data-table>
          </v-card-text>
        </v-card>

        <!-- Retention Policy -->
        <v-card class="mb-4">
          <v-card-title class="d-flex align-center">
            <v-icon class="mr-2" color="warning" icon="mdi-archive-clock" />
            Política de Retenção (GFS)
          </v-card-title>

          <v-card-text>
            <v-alert class="mb-4" density="compact" type="info" variant="tonal">
              A política de retenção é aplicada automaticamente pelo sistema.
            </v-alert>

            <v-list density="compact">
              <v-list-item>
                <template #prepend>
                  <v-chip class="mr-2" color="grey" label size="small">Horário</v-chip>
                </template>
                <v-list-item-title>
                  Mantém baseado na frequência configurada
                </v-list-item-title>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-chip class="mr-2" color="blue" label size="small">Diário</v-chip>
                </template>
                <v-list-item-title>
                  Último backup de cada dia (7 dias)
                </v-list-item-title>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-chip class="mr-2" color="purple" label size="small">Semanal</v-chip>
                </template>
                <v-list-item-title>
                  Último backup de cada semana (4 semanas)
                </v-list-item-title>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-chip class="mr-2" color="orange" label size="small">Mensal</v-chip>
                </template>
                <v-list-item-title>
                  Último backup de cada mês (12 meses)
                </v-list-item-title>
              </v-list-item>

              <v-list-item>
                <template #prepend>
                  <v-chip class="mr-2" color="red" label size="small">Anual</v-chip>
                </template>
                <v-list-item-title>
                  Último backup de cada ano (5 anos)
                </v-list-item-title>
              </v-list-item>
            </v-list>
          </v-card-text>
        </v-card>

        <!-- About -->
        <v-card>
          <v-card-title class="d-flex align-center">
            <v-icon class="mr-2" color="primary" icon="mdi-database-sync" />
            Sobre
          </v-card-title>

          <v-card-text>
            <p class="mb-4">
              <strong>DB Backup Manager</strong> é um sistema open source para
              gerenciamento de backups de múltiplos bancos de dados.
            </p>

            <v-btn
              class="mr-2"
              href="https://github.com"
              prepend-icon="mdi-github"
              target="_blank"
              variant="outlined"
            >
              GitHub
            </v-btn>

            <v-btn href="#" prepend-icon="mdi-file-document" variant="outlined">
              Documentação
            </v-btn>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <v-dialog v-model="destinationDialog" max-width="720">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon class="mr-2" color="primary" icon="mdi-folder-upload" />
          {{ editingDestinationId ? 'Editar Destino' : 'Novo Destino' }}
        </v-card-title>

        <v-card-text>
          <v-form ref="destinationFormRef" @submit.prevent="saveDestination">
            <v-row>
              <v-col cols="12" sm="8">
                <v-text-field
                  v-model="destinationForm.name"
                  label="Nome *"
                  prepend-inner-icon="mdi-label"
                  :rules="[rules.required]"
                />
              </v-col>

              <v-col cols="12" sm="4">
                <v-select
                  v-model="destinationForm.type"
                  :items="destinationTypes"
                  label="Tipo *"
                  prepend-inner-icon="mdi-shape"
                  :rules="[rules.required]"
                  @update:model-value="onDestinationTypeChange"
                />
              </v-col>

              <v-col cols="12" sm="6">
                <v-select
                  v-model="destinationForm.status"
                  :items="destinationStatusOptions"
                  label="Status"
                  prepend-inner-icon="mdi-checkbox-marked-circle-outline"
                />
              </v-col>

              <v-col cols="12" sm="6">
                <v-switch v-model="destinationForm.isDefault" color="primary" hide-details label="Destino padrão" />
              </v-col>
            </v-row>

            <v-divider class="my-6" />

            <template v-if="destinationForm.type === 'local'">
              <v-text-field
                v-model="destinationConfig.basePath"
                label="Base Path"
                placeholder="Ex: /backups"
                prepend-inner-icon="mdi-folder"
              />
            </template>

            <template v-else-if="destinationForm.type === 's3'">
              <v-row>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.region"
                    label="Região *"
                    prepend-inner-icon="mdi-earth"
                    :rules="[rules.required]"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.bucket"
                    label="Bucket *"
                    prepend-inner-icon="mdi-bucket"
                    :rules="[rules.required]"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.accessKeyId"
                    label="Access Key ID *"
                    prepend-inner-icon="mdi-key"
                    :rules="[rules.required]"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.secretAccessKey"
                    label="Secret Access Key *"
                    prepend-inner-icon="mdi-lock"
                    :rules="[rules.required]"
                    type="password"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.endpoint"
                    label="Endpoint"
                    prepend-inner-icon="mdi-link-variant"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.prefix"
                    label="Prefix"
                    prepend-inner-icon="mdi-folder-outline"
                  />
                </v-col>
                <v-col cols="12">
                  <v-switch v-model="destinationConfig.forcePathStyle" color="primary" hide-details label="Force path style" />
                </v-col>
              </v-row>
            </template>

            <template v-else-if="destinationForm.type === 'gcs'">
              <v-row>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.bucket"
                    label="Bucket *"
                    prepend-inner-icon="mdi-bucket"
                    :rules="[rules.required]"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.projectId"
                    label="Project ID"
                    prepend-inner-icon="mdi-identifier"
                  />
                </v-col>
                <v-col cols="12">
                  <v-textarea
                    v-model="destinationConfig.credentialsJson"
                    auto-grow
                    label="Credentials JSON"
                    prepend-inner-icon="mdi-file-code"
                    rows="3"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field v-model="destinationConfig.prefix" label="Prefix" prepend-inner-icon="mdi-folder-outline" />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-switch
                    v-model="destinationConfig.usingUniformAcl"
                    color="primary"
                    hide-details
                    label="Using uniform ACL"
                  />
                </v-col>
              </v-row>
            </template>

            <template v-else-if="destinationForm.type === 'azure_blob'">
              <v-row>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.container"
                    label="Container *"
                    prepend-inner-icon="mdi-package-variant"
                    :rules="[rules.required]"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field v-model="destinationConfig.prefix" label="Prefix" prepend-inner-icon="mdi-folder-outline" />
                </v-col>
                <v-col cols="12">
                  <v-textarea
                    v-model="destinationConfig.connectionString"
                    auto-grow
                    label="Connection String *"
                    prepend-inner-icon="mdi-lock"
                    :rules="[rules.required]"
                    rows="3"
                  />
                </v-col>
              </v-row>
            </template>

            <template v-else-if="destinationForm.type === 'sftp'">
              <v-row>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.host"
                    label="Host *"
                    prepend-inner-icon="mdi-server"
                    :rules="[rules.required]"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model.number="destinationConfig.port"
                    label="Porta"
                    prepend-inner-icon="mdi-ethernet"
                    :rules="[rules.portOptional]"
                    type="number"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.username"
                    label="Usuário *"
                    prepend-inner-icon="mdi-account"
                    :rules="[rules.required]"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.password"
                    label="Senha"
                    prepend-inner-icon="mdi-lock"
                    type="password"
                  />
                </v-col>
                <v-col cols="12">
                  <v-textarea
                    v-model="destinationConfig.privateKey"
                    auto-grow
                    label="Private Key"
                    prepend-inner-icon="mdi-key"
                    rows="3"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field
                    v-model="destinationConfig.passphrase"
                    label="Passphrase"
                    prepend-inner-icon="mdi-lock-outline"
                    type="password"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-text-field v-model="destinationConfig.basePath" label="Base Path" prepend-inner-icon="mdi-folder" />
                </v-col>
              </v-row>
            </template>
          </v-form>
        </v-card-text>

        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="destinationDialog = false">Cancelar</v-btn>
          <v-btn color="primary" :loading="savingDestination" type="submit" variant="flat" @click="saveDestination">
            Salvar
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <v-dialog v-model="deleteDestinationDialog" max-width="420">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon class="mr-2" color="error" icon="mdi-alert" />
          Confirmar Exclusão
        </v-card-title>

        <v-card-text>
          Tem certeza que deseja excluir o destino
          <strong>{{ destinationToDelete?.name }}</strong>?
        </v-card-text>

        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="deleteDestinationDialog = false">Cancelar</v-btn>
          <v-btn color="error" :loading="deletingDestination" variant="flat" @click="deleteDestination">
            Excluir
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script lang="ts" setup>
  import type { StorageDestination, StorageDestinationType } from '@/types/api'
  import { computed, inject, onMounted, reactive, ref } from 'vue'
  import { useTheme } from 'vuetify'
  import { ApiError, healthCheck, storageDestinationsApi } from '@/services/api'
  import { useDisplay } from 'vuetify'

  const theme = useTheme()
  const { mdAndUp } = useDisplay()
  const showNotification = inject<(msg: string, type: string) => void>('showNotification')

  const apiStatus = ref<'online' | 'offline'>('offline')
  const apiLatency = ref<number | null>(null)
  const checkingApi = ref(false)

  const storageDestinations = ref<StorageDestination[]>([])
  const loadingDestinations = ref(false)
  const destinationDialog = ref(false)
  const savingDestination = ref(false)
  const deleteDestinationDialog = ref(false)
  const deletingDestination = ref(false)
  const editingDestinationId = ref<number | null>(null)
  const destinationToDelete = ref<StorageDestination | null>(null)
  const originalDestination = ref<StorageDestination | null>(null)
  const destinationFormRef = ref()

  const settings = reactive({
    theme: 'dark' as 'light' | 'dark' | 'system',
  })

  const destinationFilters = reactive({
    search: '',
  })

  const destinationHeaders = [
    { title: 'Nome', key: 'name', sortable: true },
    { title: 'Tipo', key: 'type', sortable: true },
    { title: 'Status', key: 'status', sortable: true },
    { title: '', key: 'isDefault', sortable: false },
    { title: 'Ações', key: 'actions', sortable: false, align: 'end' as const },
  ]

  const destinationTypes = [
    { title: 'Local', value: 'local' },
    { title: 'S3/MinIO', value: 's3' },
    { title: 'Google Cloud Storage', value: 'gcs' },
    { title: 'Azure Blob', value: 'azure_blob' },
    { title: 'SFTP', value: 'sftp' },
  ]

  const destinationStatusOptions = [
    { title: 'Ativo', value: 'active' },
    { title: 'Inativo', value: 'inactive' },
  ]

  const destinationForm = reactive({
    name: '',
    type: 'local' as StorageDestinationType,
    status: 'active' as 'active' | 'inactive',
    isDefault: false,
  })

  type DestinationConfigForm = {
    basePath: string
    region: string
    bucket: string
    endpoint: string
    accessKeyId: string
    secretAccessKey: string
    forcePathStyle: boolean
    prefix: string
    projectId: string
    credentialsJson: string
    usingUniformAcl: boolean
    connectionString: string
    container: string
    host: string
    port: number
    username: string
    password: string
    privateKey: string
    passphrase: string
  }

  const destinationConfig = reactive<DestinationConfigForm>({
    basePath: '',
    region: '',
    bucket: '',
    endpoint: '',
    accessKeyId: '',
    secretAccessKey: '',
    forcePathStyle: false,
    prefix: '',
    projectId: '',
    credentialsJson: '',
    usingUniformAcl: false,
    connectionString: '',
    container: '',
    host: '',
    port: 22,
    username: '',
    password: '',
    privateKey: '',
    passphrase: '',
  })

  const rules = {
    required: (v: unknown) => {
      const value = typeof v === 'string' ? v.trim() : v
      return !!value || 'Campo obrigatório'
    },
    portOptional: (v: unknown) => {
      if (v === null || v === undefined || v === '') return true
      const value = typeof v === 'number' ? v : Number(v)
      if (Number.isNaN(value)) return 'Porta inválida'
      return (value > 0 && value <= 65_535) || 'Porta inválida'
    },
  }

  function applyTheme () {
    if (settings.theme === 'system') {
      const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
      theme.global.name.value = prefersDark ? 'dark' : 'light'
    } else {
      theme.global.name.value = settings.theme
    }

    localStorage.setItem('theme', settings.theme)
  }

  async function checkApi () {
    checkingApi.value = true
    const startTime = Date.now()

    try {
      await healthCheck()
      apiLatency.value = Date.now() - startTime
      apiStatus.value = 'online'
      showNotification?.('API está online', 'success')
    } catch {
      apiStatus.value = 'offline'
      apiLatency.value = null
      showNotification?.('API está offline', 'error')
    } finally {
      checkingApi.value = false
    }
  }

  function formatDestinationType (type: StorageDestinationType): string {
    const map: Record<StorageDestinationType, string> = {
      local: 'Local',
      s3: 'S3/MinIO',
      gcs: 'GCS',
      azure_blob: 'Azure Blob',
      sftp: 'SFTP',
    }
    return map[type] ?? type
  }

  async function loadDestinations () {
    loadingDestinations.value = true
    try {
      const response = await storageDestinationsApi.list({
        search: destinationFilters.search || undefined,
        limit: 100,
      })
      storageDestinations.value = response.data?.data ?? []
    } catch {
      storageDestinations.value = []
      showNotification?.('Erro ao carregar destinos de armazenamento', 'error')
    } finally {
      loadingDestinations.value = false
    }
  }

  let debounceTimer: ReturnType<typeof setTimeout>
  function debouncedLoadDestinations () {
    clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
      loadDestinations()
    }, 300)
  }

  function resetDestinationConfig () {
    destinationConfig.basePath = ''
    destinationConfig.region = ''
    destinationConfig.bucket = ''
    destinationConfig.endpoint = ''
    destinationConfig.accessKeyId = ''
    destinationConfig.secretAccessKey = ''
    destinationConfig.forcePathStyle = false
    destinationConfig.prefix = ''
    destinationConfig.projectId = ''
    destinationConfig.credentialsJson = ''
    destinationConfig.usingUniformAcl = false
    destinationConfig.connectionString = ''
    destinationConfig.container = ''
    destinationConfig.host = ''
    destinationConfig.port = 22
    destinationConfig.username = ''
    destinationConfig.password = ''
    destinationConfig.privateKey = ''
    destinationConfig.passphrase = ''
  }

  function onDestinationTypeChange () {
    resetDestinationConfig()
  }

  function openCreateDestination () {
    editingDestinationId.value = null
    originalDestination.value = null
    destinationForm.name = ''
    destinationForm.type = 'local'
    destinationForm.status = 'active'
    destinationForm.isDefault = false
    resetDestinationConfig()
    destinationDialog.value = true
  }

  async function openEditDestination (id: number) {
    savingDestination.value = true
    editingDestinationId.value = id
    try {
      const response = await storageDestinationsApi.get(id)
      const destination = response.data
      if (!destination) return
      originalDestination.value = destination

      destinationForm.name = destination.name
      destinationForm.type = destination.type
      destinationForm.status = destination.status
      destinationForm.isDefault = destination.isDefault
      resetDestinationConfig()

      const cfg = destination.config ?? null
      if (cfg && typeof cfg === 'object') {
        Object.assign(destinationConfig, cfg)
        if (destinationForm.type === 's3' && destinationConfig.secretAccessKey === '***') {
          destinationConfig.secretAccessKey = ''
        }
        if (destinationForm.type === 'azure_blob' && destinationConfig.connectionString === '***') {
          destinationConfig.connectionString = ''
        }
        if (destinationForm.type === 'gcs' && destinationConfig.credentialsJson === '***') {
          destinationConfig.credentialsJson = ''
        }
        if (destinationForm.type === 'sftp') {
          if (destinationConfig.password === '***') destinationConfig.password = ''
          if (destinationConfig.privateKey === '***') destinationConfig.privateKey = ''
          if (destinationConfig.passphrase === '***') destinationConfig.passphrase = ''
        }
      }

      destinationDialog.value = true
    } catch {
      showNotification?.('Erro ao carregar destino', 'error')
    } finally {
      savingDestination.value = false
    }
  }

  function stableStringify(value: unknown): string {
    if (value === undefined) return '"__undefined__"'
    if (value === null || typeof value !== 'object') return JSON.stringify(value)
    if (Array.isArray(value)) return `[${value.map(stableStringify).join(',')}]`
    const obj = value as Record<string, unknown>
    const keys = Object.keys(obj).sort()
    const entries = keys.map((k) => `${JSON.stringify(k)}:${stableStringify(obj[k])}`)
    return `{${entries.join(',')}}`
  }

  function getOriginalConfigSafe(): Record<string, unknown> {
    const cfg = originalDestination.value?.config
    if (!cfg || typeof cfg !== 'object') return {}
    const obj = cfg as Record<string, unknown>
    const { type: _type, ...rest } = obj
    return rest
  }

  const normalizedComparableConfig = computed(() => {
    const current = destinationPayloadConfig.value as Record<string, unknown>
    const original = getOriginalConfigSafe()
    const normalized: Record<string, unknown> = { ...current }

    if (destinationForm.type === 's3') {
      if (!normalized.secretAccessKey && original.secretAccessKey === '***') {
        normalized.secretAccessKey = '***'
      }
    }

    if (destinationForm.type === 'azure_blob') {
      if (!normalized.connectionString && original.connectionString === '***') {
        normalized.connectionString = '***'
      }
    }

    if (destinationForm.type === 'gcs') {
      if (!normalized.credentialsJson && original.credentialsJson === '***') {
        normalized.credentialsJson = '***'
      }
    }

    if (destinationForm.type === 'sftp') {
      if (!normalized.password && original.password === '***') normalized.password = '***'
      if (!normalized.privateKey && original.privateKey === '***') normalized.privateKey = '***'
      if (!normalized.passphrase && original.passphrase === '***') normalized.passphrase = '***'
    }

    return normalized
  })

  const destinationPayloadConfig = computed(() => {
    if (destinationForm.type === 'local') {
      const basePath = destinationConfig.basePath?.trim()
      return basePath ? { basePath } : {}
    }

    if (destinationForm.type === 's3') {
      return {
        region: destinationConfig.region,
        bucket: destinationConfig.bucket,
        endpoint: destinationConfig.endpoint || undefined,
        accessKeyId: destinationConfig.accessKeyId,
        secretAccessKey: destinationConfig.secretAccessKey,
        forcePathStyle: destinationConfig.forcePathStyle || undefined,
        prefix: destinationConfig.prefix || undefined,
      }
    }

    if (destinationForm.type === 'gcs') {
      return {
        bucket: destinationConfig.bucket,
        projectId: destinationConfig.projectId || undefined,
        credentialsJson: destinationConfig.credentialsJson || undefined,
        usingUniformAcl: destinationConfig.usingUniformAcl || undefined,
        prefix: destinationConfig.prefix || undefined,
      }
    }

    if (destinationForm.type === 'azure_blob') {
      return {
        connectionString: destinationConfig.connectionString,
        container: destinationConfig.container,
        prefix: destinationConfig.prefix || undefined,
      }
    }

    return {
      host: destinationConfig.host,
      port: destinationConfig.port || undefined,
      username: destinationConfig.username,
      password: destinationConfig.password || undefined,
      privateKey: destinationConfig.privateKey || undefined,
      passphrase: destinationConfig.passphrase || undefined,
      basePath: destinationConfig.basePath || undefined,
    }
  })

  async function saveDestination () {
    const validationResult = await destinationFormRef.value?.validate?.()
    if (validationResult && 'valid' in validationResult && validationResult.valid === false) return

    savingDestination.value = true
    try {
      if (editingDestinationId.value) {
        const original = originalDestination.value
        const originalType = original?.type

        const payload: Record<string, unknown> = {}
        if (!original || destinationForm.name.trim() !== original.name) {
          payload.name = destinationForm.name.trim()
        }
        if (!original || destinationForm.status !== original.status) {
          payload.status = destinationForm.status
        }
        if (!original || destinationForm.isDefault !== original.isDefault) {
          payload.isDefault = destinationForm.isDefault
        }

        const typeChanged = !originalType || destinationForm.type !== originalType
        const configChanged = stableStringify(normalizedComparableConfig.value) !== stableStringify(getOriginalConfigSafe())

        if (typeChanged || configChanged) {
          payload.type = destinationForm.type
          payload.config = destinationPayloadConfig.value
        }

        await storageDestinationsApi.update(editingDestinationId.value, payload)
        showNotification?.('Destino atualizado com sucesso', 'success')
      } else {
        const payload = {
          name: destinationForm.name.trim(),
          type: destinationForm.type,
          status: destinationForm.status,
          isDefault: destinationForm.isDefault,
          config: destinationPayloadConfig.value,
        }
        await storageDestinationsApi.create(payload)
        showNotification?.('Destino criado com sucesso', 'success')
      }

      destinationDialog.value = false
      loadDestinations()
    } catch (error: unknown) {
      if (error instanceof ApiError) {
        showNotification?.(error.message || 'Erro ao salvar destino', 'error')
      } else {
        showNotification?.('Erro ao salvar destino', 'error')
      }
    } finally {
      savingDestination.value = false
    }
  }

  function confirmDeleteDestination (destination: StorageDestination) {
    destinationToDelete.value = destination
    deleteDestinationDialog.value = true
  }

  async function deleteDestination () {
    if (!destinationToDelete.value) return
    deletingDestination.value = true
    try {
      await storageDestinationsApi.delete(destinationToDelete.value.id)
      showNotification?.('Destino removido com sucesso', 'success')
      deleteDestinationDialog.value = false
      destinationToDelete.value = null
      loadDestinations()
    } catch {
      showNotification?.('Erro ao remover destino', 'error')
    } finally {
      deletingDestination.value = false
    }
  }

  onMounted(() => {
    // Load saved theme
    const savedTheme = localStorage.getItem('theme') as 'light' | 'dark' | 'system' | null
    if (savedTheme) {
      settings.theme = savedTheme
      applyTheme()
    }

    // Check API status
    checkApi()

    loadDestinations()
  })
</script>
