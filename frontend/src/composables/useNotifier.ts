import { inject } from 'vue'

type NotificationType = 'success' | 'error' | 'warning' | 'info'

export function useNotifier (): (msg: string, type: NotificationType) => void {
  const showNotification = inject<(msg: string, type: NotificationType) => void>('showNotification')
  return (msg, type) => showNotification?.(msg, type)
}

