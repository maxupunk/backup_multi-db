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

export function formatDuration (seconds: number | null | undefined, emptyValue = '-'): string {
  if (seconds === null || seconds === undefined) return emptyValue
  if (seconds < 60) return `${seconds}s`

  const minutes = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${minutes}m ${secs}s`
}

