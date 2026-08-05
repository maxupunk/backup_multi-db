import { getCurrentScope, onScopeDispose } from 'vue'
import {
  type Notification,
  type NotificationCategory,
  useNotificationStore,
} from '@/stores/notification'

type NotificationListener = (notification: Notification) => void

/**
 * Registra um listener de notificações que se desregistra sozinho.
 *
 * A store de notificações é um singleton Pinia: um listener esquecido mantém
 * viva a closure do componente — e, com ela, o componente inteiro e sua subárvore
 * de DOM. Antes, cada tela precisava lembrar de chamar `offNotification` no
 * `onUnmounted`; bastava um esquecimento para vazar.
 *
 * Aqui o desregistro é amarrado ao escopo reativo do componente, então o
 * vazamento deixa de ser possível por construção.
 *
 * @example
 * useNotificationListener('backup', () => loadBackups())
 * useNotificationListener(['backup', 'restore'], handleChange)
 */
export function useNotificationListener (
  categories: NotificationCategory | '*' | Array<NotificationCategory | '*'>,
  listener: NotificationListener,
): () => void {
  const notificationStore = useNotificationStore()
  const targets = Array.isArray(categories) ? categories : [categories]

  for (const category of targets) {
    notificationStore.onNotification(category, listener)
  }

  const stop = (): void => {
    for (const category of targets) {
      notificationStore.offNotification(category, listener)
    }
  }

  if (getCurrentScope()) {
    onScopeDispose(stop)
  } else if (import.meta.env.DEV) {
    console.warn(
      '[useNotificationListener] chamado fora de um escopo reativo: '
      + 'o desregistro automático não vai acontecer, chame o retorno manualmente.',
    )
  }

  return stop
}
