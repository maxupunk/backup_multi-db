/**
 * Store para gerenciamento de notificações do sistema
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'

export type NotificationType = 'info' | 'success' | 'warning' | 'error'
export type NotificationCategory = 'system' | 'backup' | 'storage' | 'connection' | 'auth'

export interface Notification {
  id: string
  type: NotificationType
  category: NotificationCategory
  title: string
  message: string
  timestamp: string
  read: boolean
  data?: Record<string, unknown>
  timeout?: number
}

// Tipo para listeners de notificação
type NotificationListener = (notification: Notification) => void

export const useNotificationStore = defineStore('notification', () => {
  const notifications = ref<Notification[]>([])
  const history = ref<Notification[]>([])
  const unreadCount = ref(0)
  
  // Limite de histórico
  const historyLimit = 50
  
  // Listeners por categoria para permitir reação a notificações específicas
  const listeners = ref<Map<NotificationCategory | '*', Set<NotificationListener>>>(new Map())

  /**
   * Adiciona uma nova notificação
   */
  function add(notification: Omit<Notification, 'read' | 'timestamp' | 'id'> & { id?: string, timestamp?: string }) {
    const finalId = notification.id || `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`

    // Evita duplicatas se já existe notificação com mesmo ID (comum ao ouvir múltiplos canais)
    if (notifications.value.some(n => n.id === finalId) || history.value.some(n => n.id === finalId)) {
      return
    }

    const newNotification: Notification = {
      ...notification,
      id: finalId,
      timestamp: notification.timestamp || new Date().toISOString(),
      read: false,
    }


    // Adiciona à lista de exibição ativa (toasts)
    notifications.value.push(newNotification)
    
    // Adiciona ao histórico
    history.value.unshift(newNotification)
    if (history.value.length > historyLimit) {
      history.value.pop()
    }
    
    unreadCount.value++

    // Auto-remove da lista de exibição após timeout (padrão 5s, erro 10s)
    const timeoutDuration = notification.type === 'error' ? 10000 : 5000
    
    setTimeout(() => {
      remove(newNotification.id)
    }, newNotification.timeout || timeoutDuration)
    
    // Notifica listeners da categoria e listeners globais (*)
    const categoryListeners = listeners.value.get(newNotification.category)
    const globalListeners = listeners.value.get('*')
    
    if (categoryListeners) {
      categoryListeners.forEach(listener => listener(newNotification))
    }
    if (globalListeners) {
      globalListeners.forEach(listener => listener(newNotification))
    }
    
    return newNotification
  }

  /**
   * Remove uma notificação da lista ativa (fecha o toast)
   */
  function remove(id: string) {
    const index = notifications.value.findIndex(n => n.id === id)
    if (index !== -1) {
      notifications.value.splice(index, 1)
    }
  }

  /**
   * Marca uma notificação como lida
   */
  function markAsRead(id: string) {
    const notification = history.value.find(n => n.id === id)
    if (notification && !notification.read) {
      notification.read = true
      unreadCount.value = Math.max(0, unreadCount.value - 1)
    }
  }

  /**
   * Marca todas como lidas
   */
  function markAllAsRead() {
    history.value.forEach(n => n.read = true)
    unreadCount.value = 0
  }

  /**
   * Limpa o histórico
   */
  function clearHistory() {
    history.value = []
    unreadCount.value = 0
  }

  /**
   * Registra um listener para notificações de uma categoria específica
   * Use '*' para ouvir todas as categorias
   */
  function onNotification(category: NotificationCategory | '*', listener: NotificationListener) {
    if (!listeners.value.has(category)) {
      listeners.value.set(category, new Set())
    }
    listeners.value.get(category)!.add(listener)
  }

  /**
   * Remove um listener previamente registrado
   */
  function offNotification(category: NotificationCategory | '*', listener: NotificationListener) {
    const set = listeners.value.get(category)
    if (set) {
      set.delete(listener)
      if (set.size === 0) {
        listeners.value.delete(category)
      }
    }
  }

  return {
    notifications,
    history,
    unreadCount,
    add,
    remove,
    markAsRead,
    markAllAsRead,
    clearHistory,
    onNotification,
    offNotification
  }
})
