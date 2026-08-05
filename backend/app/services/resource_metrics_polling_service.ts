import logger from '@adonisjs/core/services/logger'
import transmit from '@adonisjs/transmit/services/main'
import { DockerContainerResourceEmitter } from '#services/docker_container_resource_emitter'
import { DockerContainerMonitoringService } from '#services/docker_container_monitoring_service'
import { MemoryWatermarkService } from '#services/memory_watermark_service'
import { NOTIFICATION_CHANNELS } from '#services/notification_service'
import { ResourceMetricsHistoryService } from '#services/resource_metrics_history_service'
import {
  RESOURCE_METRICS_IDLE_POLL_INTERVAL_MS,
  RESOURCE_METRICS_POLL_INTERVAL_MS,
} from '#services/resource_metrics_polling_config'
import { hasSubscribers } from '#services/sse_subscribers'
import { SystemMonitoringService } from '#services/system_monitoring_service'
import { SystemResourceEmitter } from '#services/system_resource_emitter'

const METRICS_CHANNELS = [
  NOTIFICATION_CHANNELS.SYSTEM_RESOURCES,
  NOTIFICATION_CHANNELS.DOCKER_CONTAINER_RESOURCES,
] as const

type SystemSnapshotResult = {
  resources: Awaited<ReturnType<typeof SystemMonitoringService.getResourceMetrics>>
  collectedAtIso: string
}

export class ResourceMetricsPollingService {
  private static intervalHandle: ReturnType<typeof setInterval> | null = null
  private static currentIntervalMs = RESOURCE_METRICS_POLL_INTERVAL_MS
  private static currentCyclePromise: Promise<void> | null = null
  private static unsubscribeHook: (() => void) | null = null
  private static readonly dockerMonitoringService = DockerContainerMonitoringService.instance()

  static start(): void {
    if (this.intervalHandle !== null) {
      logger.warn('[ResourceMetricsPollingService] Ja esta em execucao, ignorando start()')
      return
    }

    logger.info('[ResourceMetricsPollingService] Iniciando polling unificado de metricas')

    this.armInterval(this.resolveIntervalMs())
    this.registerSubscriptionHook()
    this.triggerCycle()
  }

  static async stop(): Promise<void> {
    this.unsubscribeHook?.()
    this.unsubscribeHook = null

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

  /**
   * Sem ninguem assinando os canais de metricas, o ciclo roda espacado: cada
   * ciclo custa duas leituras de `os.cpus()`, uma amostra de 150ms, chamadas ao
   * Docker e alocacao de payloads. O historico nao perde granularidade porque a
   * persistencia ja tem intervalo minimo de 60s.
   */
  private static resolveIntervalMs(): number {
    const watching = METRICS_CHANNELS.some((channel) => hasSubscribers(channel))

    return watching ? RESOURCE_METRICS_POLL_INTERVAL_MS : RESOURCE_METRICS_IDLE_POLL_INTERVAL_MS
  }

  private static armInterval(intervalMs: number): void {
    if (this.intervalHandle !== null) {
      if (intervalMs === this.currentIntervalMs) {
        return
      }

      clearInterval(this.intervalHandle)
    }

    this.currentIntervalMs = intervalMs
    this.intervalHandle = setInterval(() => {
      this.triggerCycle()
    }, intervalMs)
  }

  /**
   * Ao chegar um assinante, volta ao intervalo ativo e dispara um ciclo na hora
   * — sem isso o painel recem-aberto ficaria em branco ate o proximo ciclo
   * ocioso, trocando memoria por uma regressao visivel de UX.
   */
  private static registerSubscriptionHook(): void {
    if (this.unsubscribeHook) {
      return
    }

    try {
      this.unsubscribeHook = transmit.on('subscribe', ({ channel }) => {
        if (!METRICS_CHANNELS.includes(channel as (typeof METRICS_CHANNELS)[number])) {
          return
        }

        this.armInterval(RESOURCE_METRICS_POLL_INTERVAL_MS)
        this.triggerCycle()
      })
    } catch (error) {
      logger.warn(
        `[ResourceMetricsPollingService] Nao foi possivel observar assinaturas SSE: ${error}`
      )
    }
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
    MemoryWatermarkService.sample('polling')

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

    // Reavalia a cadência no fim do ciclo: se o último assinante saiu, o
    // próximo intervalo já é o ocioso.
    this.armInterval(this.resolveIntervalMs())
  }

  private static async collectSystemSnapshot(): Promise<SystemSnapshotResult> {
    const resources = await SystemMonitoringService.getResourceMetrics()

    return {
      resources,
      collectedAtIso: new Date().toISOString(),
    }
  }
}
