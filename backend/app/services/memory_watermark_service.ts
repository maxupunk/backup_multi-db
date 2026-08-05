import v8 from 'node:v8'
import logger from '@adonisjs/core/services/logger'
import { ContainerMemoryProbe } from '#services/container_memory_probe'

/**
 * Guardriail de memoria do processo, alimentado pelo ciclo de polling de metricas.
 *
 * Nao existe painel lendo estes numeros: o sinal sai pelo log. Quando a pressao
 * cruza o limiar, emite um `warn` estruturado com o uso atual e com o pico ja
 * observado desde o start — o pico e' o que separa "sempre foi assim" de
 * "acabou de subir", que e' a pergunta util na hora de investigar um OOM.
 *
 * O pico existe porque amostragem pontual nao enxerga picos curtos: um backup
 * que sobe para 300 MB por 4 segundos passaria despercebido entre dois ciclos.
 */
export class MemoryWatermarkService {
  private static readonly PRESSURE_THRESHOLD_PERCENT = 70
  private static readonly PRESSURE_LOG_COOLDOWN_MS = 60_000

  private static peakRssBytes = 0
  private static peakRssObservedAt: string | null = null
  private static peakHeapUsedBytes = 0
  private static peakHeapUsedObservedAt: string | null = null
  private static observedSince = new Date().toISOString()
  private static lastPressureLogAt = 0
  private static heapLimitBytesCache = 0

  /**
   * Atualiza os picos. Barato o suficiente para rodar em cada ciclo de polling.
   */
  static sample(context = 'polling', nowMs = Date.now()): void {
    const usage = process.memoryUsage()
    const observedAt = new Date(nowMs).toISOString()

    if (usage.rss > this.peakRssBytes) {
      this.peakRssBytes = usage.rss
      this.peakRssObservedAt = observedAt
    }

    if (usage.heapUsed > this.peakHeapUsedBytes) {
      this.peakHeapUsedBytes = usage.heapUsed
      this.peakHeapUsedObservedAt = observedAt
    }

    this.logPressureIfNeeded(context, usage, nowMs)
  }

  static getPeaks() {
    return {
      observedSince: this.observedSince,
      peakRssBytes: this.peakRssBytes,
      peakRssObservedAt: this.peakRssObservedAt,
      peakHeapUsedBytes: this.peakHeapUsedBytes,
      peakHeapUsedObservedAt: this.peakHeapUsedObservedAt,
    }
  }

  static reset(nowMs = Date.now()): void {
    this.peakRssBytes = 0
    this.peakRssObservedAt = null
    this.peakHeapUsedBytes = 0
    this.peakHeapUsedObservedAt = null
    this.observedSince = new Date(nowMs).toISOString()
    this.lastPressureLogAt = 0
  }

  /**
   * Limite de old space do V8 (definido por --max-old-space-size).
   * Nao muda durante a vida do processo, entao e' lido uma unica vez.
   */
  static getHeapLimitBytes(): number {
    if (this.heapLimitBytesCache === 0) {
      this.heapLimitBytesCache = v8.getHeapStatistics().heap_size_limit
    }

    return this.heapLimitBytesCache
  }

  private static logPressureIfNeeded(
    context: string,
    usage: NodeJS.MemoryUsage,
    nowMs: number
  ): void {
    const heapLimitBytes = this.getHeapLimitBytes()
    const memory = ContainerMemoryProbe.read(nowMs)

    const heapPercent = this.toPercent(usage.heapUsed, heapLimitBytes)
    const rssPercent = this.toPercent(usage.rss, memory.totalBytes)

    if (
      heapPercent < this.PRESSURE_THRESHOLD_PERCENT &&
      rssPercent < this.PRESSURE_THRESHOLD_PERCENT
    ) {
      return
    }

    if (nowMs - this.lastPressureLogAt < this.PRESSURE_LOG_COOLDOWN_MS) {
      return
    }

    this.lastPressureLogAt = nowMs

    logger.warn(
      {
        context,
        heapUsedBytes: usage.heapUsed,
        heapLimitBytes,
        heapPressurePercent: heapPercent,
        rssBytes: usage.rss,
        memoryLimitBytes: memory.totalBytes,
        memorySource: memory.source,
        rssPressurePercent: rssPercent,
        externalBytes: usage.external,
        arrayBuffersBytes: usage.arrayBuffers,
        ...this.getPeaks(),
      },
      `[Memory] Pressao de memoria acima de ${this.PRESSURE_THRESHOLD_PERCENT}% (heap ${heapPercent}%, rss ${rssPercent}%)`
    )
  }

  private static toPercent(value: number, total: number): number {
    if (total <= 0) {
      return 0
    }

    return Math.round(((value / total) * 100 + Number.EPSILON) * 100) / 100
  }
}
