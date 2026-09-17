/**
 * Utilitários centralizados para métricas, gráficos e status de containers Docker.
 */

export interface ContainerStateMeta {
  color: string
  icon: string
  label: string
}

export const DOCKER_CONTAINER_STATE_MAP = {
  running:    { color: 'success', icon: 'mdi-check-circle-outline', label: 'Running' },
  paused:     { color: 'warning', icon: 'mdi-pause-circle-outline', label: 'Paused' },
  restarting: { color: 'info',    icon: 'mdi-restart',              label: 'Restarting' },
  created:    { color: 'info',    icon: 'mdi-plus-circle-outline',  label: 'Created' },
  exited:     { color: 'error',   icon: 'mdi-stop-circle-outline',  label: 'Exited' },
  stopped:    { color: 'error',   icon: 'mdi-stop-circle-outline',  label: 'Stopped' },
  dead:       { color: 'error',   icon: 'mdi-skull-outline',        label: 'Dead' },
} satisfies Record<string, ContainerStateMeta>

/**
 * Retorna metadados visuais (cor, ícone, label) com base no estado do container Docker.
 */
export function resolveContainerStateMeta(status?: string | null): ContainerStateMeta {
  if (!status) {
    return { color: 'default', icon: 'mdi-help-circle-outline', label: 'Desconhecido' }
  }

  const normalized = status.toLowerCase().trim()
  const found = (DOCKER_CONTAINER_STATE_MAP as Record<string, ContainerStateMeta | undefined>)[normalized]
  if (found) {
    return found
  }

  if (normalized.includes('running') || normalized.startsWith('up')) {
    return DOCKER_CONTAINER_STATE_MAP.running
  }
  if (normalized.includes('paused')) {
    return DOCKER_CONTAINER_STATE_MAP.paused
  }
  if (normalized.includes('exited') || normalized.includes('dead') || normalized.includes('stop')) {
    return DOCKER_CONTAINER_STATE_MAP.exited
  }

  return { color: 'default', icon: 'mdi-help-circle-outline', label: status }
}

/**
 * Retorna a cor correspondente ao estado operacional de um contêiner Docker.
 */
export function resolveContainerStatusColor(status?: string | null): string {
  return resolveContainerStateMeta(status).color
}

/**
 * Retorna a cor semântica do Vuetify com base no percentual de consumo de recursos.
 * - < 65%: 'success' (verde)
 * - 65% a 84.9%: 'warning' (amarelo/laranja)
 * - >= 85%: 'error' (vermelho)
 */
export function resolveUsageColor(percentage: number): 'success' | 'warning' | 'error' {
  if (percentage >= 85) return 'error'
  if (percentage >= 65) return 'warning'
  return 'success'
}

/**
 * Retorna a cor CSS (RGB) correspondente à cor semântica do Vuetify para uso em SVGs e Canvas.
 */
export function resolveChartColor(color: string): string {
  if (color === 'error') return 'rgb(var(--v-theme-error))'
  if (color === 'warning') return 'rgb(var(--v-theme-warning))'
  if (color === 'success') return 'rgb(var(--v-theme-success))'
  return 'rgb(var(--v-theme-primary))'
}
