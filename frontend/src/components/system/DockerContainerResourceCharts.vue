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

          <div class="group-totals" aria-label="Totais de recursos do grupo">
            <div
              class="group-total"
              :style="{ '--group-total-color': resolveThemeColor(group.totalCpuUsagePercent) }"
            >
              <span class="group-total__dot" aria-hidden="true" />
              <span class="group-total__label">CPU total</span>
              <strong class="group-total__value">{{ formatCpuPercent(group.totalCpuUsagePercent) }}</strong>
            </div>

            <div
              class="group-total"
              :style="{ '--group-total-color': resolveThemeColor(group.totalMemoryUsagePercent) }"
            >
              <span class="group-total__dot" aria-hidden="true" />
              <span class="group-total__label">Memória total</span>
              <strong class="group-total__value">{{ formatGroupMemory(group) }}</strong>
            </div>
          </div>
        </div>

        <v-row>
          <v-col
            v-for="container in group.containers"
            :key="container.containerId"
            cols="12"
            :md="group.containers.length === 1 ? 12 : 6"
          >
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
import { formatBytes } from '@/utils/format'
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
  totalCpuUsagePercent: number
  totalMemoryUsageBytes: number
  effectiveMemoryLimitBytes: number
  totalMemoryUsagePercent: number
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
      totalCpuUsagePercent: 0,
      totalMemoryUsageBytes: 0,
      effectiveMemoryLimitBytes: 0,
      totalMemoryUsagePercent: 0,
    })
  }

  return Array.from(groups.values())
    .map((group) => {
      const sortedContainers = [...group.containers].sort((a, b) => a.containerName.localeCompare(b.containerName))
      const totalCpuUsagePercent = sortedContainers.reduce((total, container) => total + container.cpu.usagePercent, 0)
      const totalMemoryUsageBytes = sortedContainers.reduce((total, container) => total + container.memory.usageBytes, 0)
      const effectiveMemoryLimitBytes = sortedContainers.reduce(
        (highestLimit, container) => Math.max(highestLimit, container.memory.limitBytes),
        0,
      )

      return {
        ...group,
        containers: sortedContainers,
        totalCpuUsagePercent,
        totalMemoryUsageBytes,
        effectiveMemoryLimitBytes,
        totalMemoryUsagePercent:
          effectiveMemoryLimitBytes > 0 ? (totalMemoryUsageBytes / effectiveMemoryLimitBytes) * 100 : 0,
      }
    })
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

function formatCpuPercent(value: number): string {
  return `${value.toFixed(1)}%`
}

function formatGroupMemory(group: ContainerGroup): string {
  if (group.effectiveMemoryLimitBytes > 0) {
    return `${formatBytes(group.totalMemoryUsageBytes)} / ${formatBytes(group.effectiveMemoryLimitBytes)}`
  }

  return formatBytes(group.totalMemoryUsageBytes)
}

function resolveUsageColor(percentage: number): string {
  if (percentage >= 85) return 'error'
  if (percentage >= 65) return 'warning'
  return 'success'
}

function resolveThemeColor(percentage: number): string {
  const color = resolveUsageColor(percentage)

  if (color === 'error') return 'rgb(var(--v-theme-error))'
  if (color === 'warning') return 'rgb(var(--v-theme-warning))'
  return 'rgb(var(--v-theme-success))'
}
</script>

<style scoped>
.project-group {
  padding-top: 4px;
}

.group-totals {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 18px;
  align-items: center;
}

.group-total {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.group-total__dot {
  width: 8px;
  height: 8px;
  flex: 0 0 auto;
  border-radius: 999px;
  background: var(--group-total-color);
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--group-total-color) 16%, transparent);
}

.group-total__label {
  font-size: 0.75rem;
  line-height: 1;
  color: rgba(var(--v-theme-on-surface), 0.62);
}

.group-total__value {
  font-size: 0.82rem;
  color: var(--group-total-color);
}

@media (max-width: 960px) {
  .group-totals {
    justify-content: flex-start;
  }
}
</style>
