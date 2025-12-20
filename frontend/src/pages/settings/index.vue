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
  </div>
</template>

<script lang="ts" setup>
  import { inject, onMounted, reactive, ref } from 'vue'
  import { useTheme } from 'vuetify'
  import { healthCheck } from '@/services/api'

  const theme = useTheme()
  const showNotification = inject<(msg: string, type: string) => void>('showNotification')

  const apiStatus = ref<'online' | 'offline'>('offline')
  const apiLatency = ref<number | null>(null)
  const checkingApi = ref(false)

  const settings = reactive({
    theme: 'dark' as 'light' | 'dark' | 'system',
  })

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

  onMounted(() => {
    // Load saved theme
    const savedTheme = localStorage.getItem('theme') as 'light' | 'dark' | 'system' | null
    if (savedTheme) {
      settings.theme = savedTheme
      applyTheme()
    }

    // Check API status
    checkApi()
  })
</script>
