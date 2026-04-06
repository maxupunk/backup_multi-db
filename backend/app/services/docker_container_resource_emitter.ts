import transmit from '@adonisjs/transmit/services/main'
import logger from '@adonisjs/core/services/logger'
import { NOTIFICATION_CHANNELS } from '#services/notification_service'
import { DockerContainerMonitoringService } from '#services/docker_container_monitoring_service'

type BroadcastableValue =
  | { [key: string]: BroadcastableValue }
  | string
  | number
  | boolean
  | null
  | BroadcastableValue[]

/**
 * Emite métricas de recursos dos containers Docker via SSE.
 */
export class DockerContainerResourceEmitter {
  private static readonly INTERVAL_MS = 5_000
  private static intervalHandle: ReturnType<typeof setInterval> | null = null
  private static readonly monitoringService = new DockerContainerMonitoringService()

  static start(): void {
    if (this.intervalHandle !== null) {
      logger.warn('[DockerContainerResourceEmitter] Ja esta em execucao, ignorando start()')
      return
    }

    logger.info('[DockerContainerResourceEmitter] Iniciando broadcast de recursos dos containers')

    this.intervalHandle = setInterval(() => {
      void this.broadcastMetrics()
    }, this.INTERVAL_MS)

    void this.broadcastMetrics()
  }

  static stop(): void {
    if (this.intervalHandle === null) {
      return
    }

    clearInterval(this.intervalHandle)
    this.intervalHandle = null
    logger.info('[DockerContainerResourceEmitter] Broadcast de recursos encerrado')
  }

  private static async broadcastMetrics(): Promise<void> {
    try {
      const overview = await this.monitoringService.getOverview()

      const payload: { [key: string]: BroadcastableValue } = {
        dockerAvailable: overview.dockerAvailable,
        unavailableReason: overview.unavailableReason,
        collectedAt: overview.collectedAt,
        containers: overview.containers.map((container) => ({
          containerId: container.containerId,
          containerName: container.containerName,
          imageName: container.imageName,
          status: container.status,
          cpu: {
            usagePercent: container.cpu.usagePercent,
          },
          memory: {
            usageBytes: container.memory.usageBytes,
            limitBytes: container.memory.limitBytes,
            usagePercent: container.memory.usagePercent,
          },
          network: {
            rxBytes: container.network.rxBytes,
            txBytes: container.network.txBytes,
          },
          blockIo: {
            readBytes: container.blockIo.readBytes,
            writeBytes: container.blockIo.writeBytes,
          },
          pids: container.pids,
        })),
      }

      transmit.broadcast(NOTIFICATION_CHANNELS.DOCKER_CONTAINER_RESOURCES, payload)
    } catch (error) {
      logger.error(`[DockerContainerResourceEmitter] Erro ao emitir metricas: ${error}`)
    }
  }
}
