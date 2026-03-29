<template>
  <v-card>
    <v-card-title class="d-flex align-center justify-space-between">
      <div class="d-flex align-center">
        <v-icon class="mr-2" color="info" icon="mdi-information" />
        Informações do Sistema
      </div>

      <v-btn
        color="info"
        :loading="checking"
        prepend-icon="mdi-refresh"
        variant="tonal"
        @click="emit('refresh')"
      >
        Verificar conexão
      </v-btn>
    </v-card-title>

    <v-card-text>
      <v-list density="compact">
        <v-list-item>
          <template #prepend>
            <v-icon icon="mdi-tag" />
          </template>
          <v-list-item-title>Versão</v-list-item-title>
          <template #append>
            <span class="text-medium-emphasis">{{ system?.version ?? '-' }}</span>
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

        <v-list-item>
          <template #prepend>
            <v-icon icon="mdi-cog-sync" />
          </template>
          <v-list-item-title>Status do Work/Jobs</v-list-item-title>
          <template #append>
            <v-chip :color="jobsStatusColor" label size="small">
              {{ jobsStatusLabel }}
            </v-chip>
          </template>
        </v-list-item>

        <v-list-item>
          <template #prepend>
            <v-icon icon="mdi-format-list-numbered" />
          </template>
          <v-list-item-title>Jobs Ativos</v-list-item-title>
          <template #append>
            <span class="text-medium-emphasis">{{ system?.jobs.activeJobs ?? 0 }}</span>
          </template>
        </v-list-item>

        <v-list-item>
          <template #prepend>
            <v-icon icon="mdi-speedometer" />
          </template>
          <v-list-item-title>Latência</v-list-item-title>
          <template #append>
            <span class="text-medium-emphasis">{{ latencyLabel }}</span>
          </template>
        </v-list-item>

        <v-list-item>
          <template #prepend>
            <v-icon icon="mdi-laptop" />
          </template>
          <v-list-item-title>Host</v-list-item-title>
          <template #append>
            <span class="text-medium-emphasis">{{ system?.hostname ?? '-' }}</span>
          </template>
        </v-list-item>

        <v-list-item>
          <template #prepend>
            <v-icon icon="mdi-microsoft-windows" />
          </template>
          <v-list-item-title>Plataforma</v-list-item-title>
          <template #append>
            <span class="text-medium-emphasis">{{ platformLabel }}</span>
          </template>
        </v-list-item>

        <v-list-item>
          <template #prepend>
            <v-icon icon="mdi-language-javascript" />
          </template>
          <v-list-item-title>Node.js</v-list-item-title>
          <template #append>
            <span class="text-medium-emphasis">{{ system?.nodeVersion ?? '-' }}</span>
          </template>
        </v-list-item>

        <v-list-item>
          <template #prepend>
            <v-icon icon="mdi-timer-outline" />
          </template>
          <v-list-item-title>Uptime</v-list-item-title>
          <template #append>
            <span class="text-medium-emphasis">{{ uptimeLabel }}</span>
          </template>
        </v-list-item>
      </v-list>
    </v-card-text>
  </v-card>
</template>

<script lang="ts" setup>
import type { SystemStatus } from '@/types/api'
import { computed } from 'vue'
import { formatDuration } from '@/utils/format'

const props = defineProps<{
  system: SystemStatus | null
  apiStatus: 'online' | 'offline'
  apiLatency: number | null
  checking: boolean
}>()

const emit = defineEmits<{
  refresh: []
}>()

const jobsStatusColor = computed(() => {
  if (!props.system) return 'grey'
  return props.system.jobs.status === 'ok' && props.system.jobs.isRunning ? 'success' : 'error'
})

const jobsStatusLabel = computed(() => {
  if (!props.system) return 'Desconhecido'
  return props.system.jobs.status === 'ok' && props.system.jobs.isRunning ? 'Rodando e OK' : 'Parado/Erro'
})

const latencyLabel = computed(() => {
  if (props.apiStatus === 'offline') return '-'
  return props.apiLatency !== null ? `${props.apiLatency}ms` : 'Verificando...'
})

const platformLabel = computed(() => {
  if (!props.system) return '-'
  return `${props.system.platform} (${props.system.architecture})`
})

const uptimeLabel = computed(() => formatDuration(props.system?.uptimeSeconds))
</script>
