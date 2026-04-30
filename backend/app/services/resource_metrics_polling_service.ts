import logger from '@adonisjs/core/services/logger'
import { DockerContainerResourceEmitter } from '#services/docker_container_resource_emitter'
import { DockerContainerMonitoringService } from '#services/docker_container_monitoring_service'
import { ResourceMetricsHistoryService } from '#services/resource_metrics_history_service'
import { RESOURCE_METRICS_POLL_INTERVAL_MS } from '#services/resource_metrics_polling_config'
import { SystemMonitoringService } from '#services/system_monitoring_service'
import { SystemResourceEmitter } from '#services/system_resource_emitter'

type SystemSnapshotResult = {
  resources: Awaited<ReturnType<typeof SystemMonitoringService.getResourceMetrics>>
  collectedAtIso: string
}

export class ResourceMetricsPollingService {
  private static readonly INTERVAL_MS = RESOURCE_METRICS_POLL_INTERVAL_MS
  private static intervalHandle: ReturnType<typeof setInterval> | null = null
  private static currentCyclePromise: Promise<void> | null = null
  private static readonly dockerMonitoringService = DockerContainerMonitoringService.instance()

  static start(): void {
    if (this.intervalHandle !== null) {
      logger.warn('[ResourceMetricsPollingService] Ja esta em execucao, ignorando start()')
      return
    }

    logger.info('[ResourceMetricsPollingService] Iniciando polling unificado de metricas')

    this.intervalHandle = setInterval(() => {
      this.triggerCycle()
    }, this.INTERVAL_MS)

    this.triggerCycle()
  }

  static async stop(): Promise<void> {
    if (this.intervalHandle === null) {
      await ResourceMetricsHistoryService.flushPendingRows()
      return
    }

    clearInterval(this.intervalHandle)
    this.intervalHandle = null

    if (this.currentCyclePromise) {
      await this.currentCyclePromise
    }

    await ResourceMetricsHistoryService.flushPendingRows()
    logger.info('[ResourceMetricsPollingService] Polling unificado de metricas encerrado')
  }

  private static triggerCycle(): void {
    if (this.currentCyclePromise) {
      return
    }

    this.currentCyclePromise = this.runCycle().finally(() => {
      this.currentCyclePromise = null
    })
  }

  private static async runCycle(): Promise<void> {
    const [systemResult, dockerResult] = await Promise.allSettled([
      this.collectSystemSnapshot(),
      this.dockerMonitoringService.getOverview(),
    ])

    if (systemResult.status === 'fulfilled') {
      const { resources, collectedAtIso } = systemResult.value

      try {
        await ResourceMetricsHistoryService.recordSystemSnapshot(resources, collectedAtIso)
      } catch (error) {
        logger.error(
          `[ResourceMetricsPollingService] Falha ao persistir metricas do sistema: ${error}`
        )
      }

      SystemResourceEmitter.broadcast(resources, collectedAtIso)
    } else {
      logger.error(
        `[ResourceMetricsPollingService] Falha ao coletar metricas do sistema: ${systemResult.reason}`
      )
    }

    if (dockerResult.status === 'fulfilled') {
      try {
        await ResourceMetricsHistoryService.recordContainerSnapshot(dockerResult.value)
      } catch (error) {
        logger.error(`[ResourceMetricsPollingService] Falha ao persistir metricas Docker: ${error}`)
      }

      DockerContainerResourceEmitter.broadcast(dockerResult.value)
    } else {
      logger.error(
        `[ResourceMetricsPollingService] Falha ao coletar metricas Docker: ${dockerResult.reason}`
      )
    }
  }

  private static async collectSystemSnapshot(): Promise<SystemSnapshotResult> {
    const resources = await SystemMonitoringService.getResourceMetrics()

    return {
      resources,
      collectedAtIso: new Date().toISOString(),
    }
  }
}
