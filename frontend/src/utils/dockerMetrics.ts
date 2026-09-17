/**
 * Utilitários centralizados para métricas, gráficos e status de containers Docker.
 */

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

/**
 * Retorna a cor correspondente ao estado operacional de um contêiner Docker.
 */
export function resolveContainerStatusColor(status: string): string {
  const normalized = status.toLowerCase()
  if (normalized.includes('running') || normalized === 'up') return 'success'
  if (normalized.includes('paused')) return 'warning'
  if (normalized.includes('exited') || normalized.includes('dead')) return 'error'
  return 'primary'
}
