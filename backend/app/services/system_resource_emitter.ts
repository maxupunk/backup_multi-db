import transmit from '@adonisjs/transmit/services/main'
import logger from '@adonisjs/core/services/logger'
import { NOTIFICATION_CHANNELS } from '#services/notification_service'
import type { SystemResourceMetrics } from '#services/system_monitoring_service'

/**
 * Tipo compatível com Broadcastable do Transmit (recursivo)
 */
type BroadcastableValue =
  | { [key: string]: BroadcastableValue }
  | string
  | number
  | boolean
  | null
  | BroadcastableValue[]

/**
 * Responsabilidade única: converter métricas de sistema em payload SSE e
 * publicar no canal de recursos do sistema.
 */
export class SystemResourceEmitter {
  static broadcast(resources: SystemResourceMetrics, collectedAtIso: string): void {
    try {
      const payload: { [key: string]: BroadcastableValue } = {
        cpu: {
          usagePercent: resources.cpu.usagePercent,
          cores: resources.cpu.cores,
          model: resources.cpu.model,
        },
        memory: {
          totalBytes: resources.memory.totalBytes,
          usedBytes: resources.memory.usedBytes,
          freeBytes: resources.memory.freeBytes,
          usagePercent: resources.memory.usagePercent,
        },
        timestamp: collectedAtIso,
      }

      transmit.broadcast(NOTIFICATION_CHANNELS.SYSTEM_RESOURCES, payload)
    } catch (error) {
      logger.error(`[SystemResourceEmitter] Erro ao emitir métricas: ${error}`)
    }
  }
}
