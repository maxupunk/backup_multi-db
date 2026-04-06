import transmit from '@adonisjs/transmit/services/main'
import logger from '@adonisjs/core/services/logger'
import { NOTIFICATION_CHANNELS } from '#services/notification_service'
import { ResourceMetricsHistoryService } from '#services/resource_metrics_history_service'
import { SystemMonitoringService } from '#services/system_monitoring_service'

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
 * Emite métricas de CPU e RAM via SSE a cada ~1 segundo.
 *
 * Responsabilidade única: fazer polling das métricas de sistema e
 * broadcastar para o canal `notifications/system-resources`.
 *
 * O ciclo de vida (start/stop) deve ser gerenciado pelo arquivo de boot
 * da aplicação (start/transmit.ts).
 */
export class SystemResourceEmitter {
  private static readonly INTERVAL_MS = 1_000
  private static intervalHandle: ReturnType<typeof setInterval> | null = null

  /**
   * Inicia o broadcast periódico de métricas. Idempotente: chamadas
   * duplicadas são ignoradas caso o emitter já esteja rodando.
   */
  static start(): void {
    if (this.intervalHandle !== null) {
      logger.warn('[SystemResourceEmitter] Já está em execução, ignorando start()')
      return
    }

    logger.info('[SystemResourceEmitter] Iniciando broadcast de recursos do sistema')

    this.intervalHandle = setInterval(() => {
      void this.broadcastMetrics()
    }, this.INTERVAL_MS)
  }

  /**
   * Para o broadcast periódico e libera o timer.
   */
  static stop(): void {
    if (this.intervalHandle === null) {
      return
    }

    clearInterval(this.intervalHandle)
    this.intervalHandle = null
    logger.info('[SystemResourceEmitter] Broadcast de recursos encerrado')
  }

  /**
   * Coleta as métricas e faz o broadcast via SSE.
   * Erros são capturados para não derrubar o intervalo.
   */
  private static async broadcastMetrics(): Promise<void> {
    try {
      const resources = await SystemMonitoringService.getResourceMetrics()

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
        timestamp: new Date().toISOString(),
      }

      await ResourceMetricsHistoryService.recordSystemSnapshot(resources, String(payload.timestamp))

      transmit.broadcast(NOTIFICATION_CHANNELS.SYSTEM_RESOURCES, payload)
    } catch (error) {
      logger.error(`[SystemResourceEmitter] Erro ao emitir métricas: ${error}`)
    }
  }
}
