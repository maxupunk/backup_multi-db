export function formatDateTimePtBR (
  dateString?: string | null,
  options: { withYear?: boolean } = {},
): string {
  if (!dateString) return '-'
  const { withYear = true } = options

  const date = new Date(dateString)
  return date.toLocaleString('pt-BR', {
    day: '2-digit',
    month: '2-digit',
    ...(withYear ? { year: 'numeric' as const } : {}),
    hour: '2-digit',
    minute: '2-digit',
  })
}

export function formatFileSize (bytes: number | null | undefined, emptyValue = 'N/A'): string {
  if (bytes === null || bytes === undefined) return emptyValue

  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let size = bytes
  let unitIndex = 0

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024
    unitIndex++
  }

  return `${size.toFixed(1)} ${units[unitIndex]}`
}

export function formatBytes (bytes: number | null | undefined, emptyValue = '-'): string {
  if (bytes === null || bytes === undefined) return emptyValue

  return formatFileSize(bytes, emptyValue)
}

export function formatDuration (seconds: number | null | undefined, emptyValue = '-'): string {
  if (seconds === null || seconds === undefined) return emptyValue
  if (seconds < 60) return `${seconds}s`

  const days = Math.floor(seconds / 86_400)
  const hours = Math.floor((seconds % 86_400) / 3_600)
  const minutes = Math.floor((seconds % 3_600) / 60)
  const secs = seconds % 60
  const segments = [
    days ? `${days}d` : null,
    hours ? `${hours}h` : null,
    minutes ? `${minutes}m` : null,
    secs || segmentsAreEmpty(days, hours, minutes) ? `${secs}s` : null,
  ].filter(Boolean)

  return segments.join(' ')
}

function segmentsAreEmpty (...values: number[]): boolean {
  return values.every(value => value === 0)
}
