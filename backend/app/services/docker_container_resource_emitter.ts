import transmit from '@adonisjs/transmit/services/main'
import logger from '@adonisjs/core/services/logger'
import { NOTIFICATION_CHANNELS } from '#services/notification_service'
import type { DockerContainerResourceOverview } from '#services/docker_container_monitoring_service'

type BroadcastableValue =
  | { [key: string]: BroadcastableValue }
  | string
  | number
  | boolean
  | null
  | BroadcastableValue[]

/**
 * Responsabilidade única: converter métricas de containers Docker em payload
 * SSE e publicar no canal de recursos dos containers.
 */
export class DockerContainerResourceEmitter {
  static broadcast(overview: DockerContainerResourceOverview): void {
    try {
      const payload: { [key: string]: BroadcastableValue } = {
        dockerAvailable: overview.dockerAvailable,
        unavailableReason: overview.unavailableReason,
        collectedAt: overview.collectedAt,
        containers: overview.containers.map((container) => ({
          containerId: container.containerId,
          containerName: container.containerName,
          projectName: container.projectName,
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
