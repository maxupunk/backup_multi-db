<template>
  <v-row class="mb-6">
    <v-col cols="12">
      <div class="d-flex align-center mb-4">
        <v-icon class="mr-2" color="primary" icon="mdi-docker" />
        <h2 class="text-h6 font-weight-medium">Recursos por Contêiner</h2>
      </div>
    </v-col>

    <v-col v-if="error" cols="12">
      <v-alert type="error" variant="tonal">
        {{ error }}
      </v-alert>
    </v-col>

    <v-col v-else-if="!overview?.dockerAvailable" cols="12">
      <v-alert type="warning" variant="tonal">
        Docker indisponível para coleta de métricas.
        <div v-if="overview?.unavailableReason" class="mt-1 text-caption">
          {{ overview.unavailableReason }}
        </div>
      </v-alert>
    </v-col>

    <v-col v-else-if="loading && !overview" cols="12">
      <v-skeleton-loader type="card@2" />
    </v-col>

    <v-col v-else-if="!groupedContainers.length" cols="12">
      <v-alert type="info" variant="tonal">
        Nenhum contêiner em execução no momento.
      </v-alert>
    </v-col>

    <v-col v-for="group in groupedContainers" :key="group.key" cols="12">
      <section class="project-group">
        <div class="d-flex flex-column flex-md-row align-md-center justify-space-between ga-3 mb-4">
          <div>
            <div class="d-flex align-center flex-wrap ga-2 mb-1">
              <v-icon color="primary" icon="mdi-folder-multiple-outline" size="20" />
              <h3 class="text-subtitle-1 font-weight-bold mb-0">
                {{ group.label }}
              </h3>
              <v-chip color="primary" label size="small" variant="tonal">
                {{ group.containers.length }}
              </v-chip>
            </div>

            <p class="text-caption text-medium-emphasis mb-0">
              {{ group.description }}
            </p>
          </div>
        </div>

        <v-row>
          <v-col v-for="container in group.containers" :key="container.containerId" cols="12" md="6">
            <DockerContainerResourceCard
              :container="container"
              :history-points="resolveContainerHistoryPoints(container.containerId)"
              :range-hours="rangeHours"
            />
          </v-col>
        </v-row>
      </section>
    </v-col>
  </v-row>
</template>

<script lang="ts" setup>
import type { ContainerResourceHistory, DockerContainerResourceMetrics, DockerContainerResourceOverview, ResourceHistoryPoint } from '@/types/api'
import { computed } from 'vue'
import DockerContainerResourceCard from './DockerContainerResourceCard.vue'

const props = defineProps<{
  overview: DockerContainerResourceOverview | null
  historyByContainerId: Record<string, ContainerResourceHistory>
  loading: boolean
  error: string | null
  rangeHours?: number
}>()

const containers = computed(() => props.overview?.containers ?? [])

type ContainerGroup = {
  key: string
  label: string
  description: string
  containers: DockerContainerResourceMetrics[]
  unnamed: boolean
}

const groupedContainers = computed<ContainerGroup[]>(() => {
  const groups = new Map<string, ContainerGroup>()

  for (const container of containers.value) {
    const projectName = container.projectName?.trim() || null
    const key = projectName?.toLocaleLowerCase() || '__sem-projeto__'
    const existing = groups.get(key)

    if (existing) {
      existing.containers.push(container)
      continue
    }

    groups.set(key, {
      key,
      label: projectName || 'Sem projeto identificado',
      description: projectName
        ? 'Contêineres agrupados pelo mesmo projeto Docker.'
        : 'Contêineres sem label ou convenção clara de projeto.',
      containers: [container],
      unnamed: !projectName,
    })
  }

  return Array.from(groups.values())
    .map((group) => ({
      ...group,
      containers: [...group.containers].sort((a, b) => a.containerName.localeCompare(b.containerName)),
    }))
    .sort((a, b) => {
      if (a.unnamed !== b.unnamed) {
        return a.unnamed ? 1 : -1
      }

      return a.label.localeCompare(b.label)
    })
})

function resolveContainerHistoryPoints(containerId: string): ResourceHistoryPoint[] {
  return props.historyByContainerId[containerId]?.points ?? []
}
</script>

<style scoped>
.project-group {
  padding-top: 4px;
}
</style>
