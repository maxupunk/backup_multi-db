import { readFileSync } from 'node:fs'
import os from 'node:os'

export type MemoryMetricsSource = 'cgroup-v2' | 'cgroup-v1' | 'os'

export type MemoryProbeReading = {
  source: MemoryMetricsSource
  containerLimited: boolean
  totalBytes: number
  usedBytes: number
  freeBytes: number
}

const CGROUP_V2_LIMIT_PATH = '/sys/fs/cgroup/memory.max'
const CGROUP_V2_USAGE_PATH = '/sys/fs/cgroup/memory.current'
const CGROUP_V2_STAT_PATH = '/sys/fs/cgroup/memory.stat'

const CGROUP_V1_LIMIT_PATH = '/sys/fs/cgroup/memory/memory.limit_in_bytes'
const CGROUP_V1_USAGE_PATH = '/sys/fs/cgroup/memory/memory.usage_in_bytes'
const CGROUP_V1_STAT_PATH = '/sys/fs/cgroup/memory/memory.stat'

/**
 * O cgroup v1 grava 0x7FFFFFFFFFFFF000 quando nao ha limite configurado.
 * Qualquer valor acima de 2^53 e' "sem limite" na pratica.
 */
const UNLIMITED_THRESHOLD_BYTES = 2 ** 53

/**
 * Converte o conteudo de memory.max / memory.limit_in_bytes em bytes.
 * Retorna null quando o cgroup nao impoe limite ("max" no v2, sentinela no v1).
 */
export function parseCgroupLimit(raw: string): number | null {
  const value = raw.trim()

  if (!value || value === 'max') {
    return null
  }

  const parsed = Number(value)

  if (!Number.isFinite(parsed) || parsed <= 0 || parsed >= UNLIMITED_THRESHOLD_BYTES) {
    return null
  }

  return parsed
}

/**
 * Converte o conteudo de memory.current / memory.usage_in_bytes em bytes.
 */
export function parseCgroupUsage(raw: string): number | null {
  const parsed = Number(raw.trim())

  if (!Number.isFinite(parsed) || parsed < 0) {
    return null
  }

  return parsed
}

/**
 * Extrai o page cache inativo de memory.stat.
 *
 * O `docker stats` desconta esse valor do uso bruto porque page cache inativo
 * e' recuperavel sob pressao — sem o desconto, o numero exibido fica bem acima
 * do que o OOM killer realmente considera.
 *
 * cgroup v2 usa a chave `inactive_file`; o v1 usa `total_inactive_file`.
 */
export function parseInactiveFileBytes(raw: string): number {
  for (const line of raw.split('\n')) {
    const [key, value] = line.trim().split(/\s+/)

    if (key !== 'inactive_file' && key !== 'total_inactive_file') {
      continue
    }

    const parsed = Number(value)

    if (Number.isFinite(parsed) && parsed >= 0) {
      return parsed
    }
  }

  return 0
}

/**
 * Le a memoria disponivel para o processo respeitando limites de container.
 *
 * `os.totalmem()` / `os.freemem()` reportam a memoria do HOST — dentro de um
 * container com `limits.memory` isso nao tem relacao com o que o OOM killer
 * enxerga. Este probe le o cgroup quando ele existe e so cai para `os.*`
 * fora de container.
 */
export class ContainerMemoryProbe {
  private static readonly READING_TTL_MS = 1_000

  private static detectedSource: MemoryMetricsSource | null = null
  private static detectedLimitBytes: number | null = null
  private static cachedReading: MemoryProbeReading | null = null
  private static cachedReadingAt = 0

  static read(nowMs = Date.now()): MemoryProbeReading {
    if (this.cachedReading && nowMs - this.cachedReadingAt < this.READING_TTL_MS) {
      return this.cachedReading
    }

    const reading = this.readFromSource()

    this.cachedReading = reading
    this.cachedReadingAt = nowMs

    return reading
  }

  /**
   * Limpa a deteccao e o cache. Usado apenas por testes.
   */
  static reset(): void {
    this.detectedSource = null
    this.detectedLimitBytes = null
    this.cachedReading = null
    this.cachedReadingAt = 0
  }

  private static readFromSource(): MemoryProbeReading {
    if (this.detectedSource === null) {
      this.detectSource()
    }

    if (this.detectedSource === 'cgroup-v2') {
      const reading = this.readCgroup('cgroup-v2', CGROUP_V2_USAGE_PATH, CGROUP_V2_STAT_PATH)
      if (reading) return reading
    }

    if (this.detectedSource === 'cgroup-v1') {
      const reading = this.readCgroup('cgroup-v1', CGROUP_V1_USAGE_PATH, CGROUP_V1_STAT_PATH)
      if (reading) return reading
    }

    return this.readFromOs()
  }

  /**
   * Determina a fonte uma unica vez: os arquivos de cgroup nao aparecem nem
   * somem durante a vida do processo, entao nao ha motivo para tentar a cada
   * ciclo de polling (no Windows isso seria um ENOENT a cada 10s).
   */
  private static detectSource(): void {
    const v2Limit = this.readLimit(CGROUP_V2_LIMIT_PATH)

    if (v2Limit !== undefined) {
      this.detectedSource = 'cgroup-v2'
      this.detectedLimitBytes = v2Limit
      return
    }

    const v1Limit = this.readLimit(CGROUP_V1_LIMIT_PATH)

    if (v1Limit !== undefined) {
      this.detectedSource = 'cgroup-v1'
      this.detectedLimitBytes = v1Limit
      return
    }

    this.detectedSource = 'os'
    this.detectedLimitBytes = null
  }

  /**
   * Retorna `undefined` quando o arquivo nao existe (cgroup indisponivel) e
   * `null` quando existe mas nao impoe limite.
   */
  private static readLimit(path: string): number | null | undefined {
    const raw = this.readFileOrNull(path)

    if (raw === null) {
      return undefined
    }

    const limit = parseCgroupLimit(raw)

    if (limit === null) {
      return null
    }

    // Um limite maior que a RAM do host nao restringe nada na pratica.
    return limit >= os.totalmem() ? null : limit
  }

  private static readCgroup(
    source: MemoryMetricsSource,
    usagePath: string,
    statPath: string
  ): MemoryProbeReading | null {
    const limitBytes = this.detectedLimitBytes

    if (limitBytes === null) {
      return null
    }

    const rawUsage = this.readFileOrNull(usagePath)

    if (rawUsage === null) {
      return null
    }

    const usage = parseCgroupUsage(rawUsage)

    if (usage === null) {
      return null
    }

    const rawStat = this.readFileOrNull(statPath)
    const inactiveFile = rawStat === null ? 0 : parseInactiveFileBytes(rawStat)
    const usedBytes = Math.max(0, Math.min(usage - inactiveFile, limitBytes))

    return {
      source,
      containerLimited: true,
      totalBytes: limitBytes,
      usedBytes,
      freeBytes: Math.max(0, limitBytes - usedBytes),
    }
  }

  private static readFromOs(): MemoryProbeReading {
    const totalBytes = os.totalmem()
    const freeBytes = os.freemem()

    return {
      source: 'os',
      containerLimited: false,
      totalBytes,
      usedBytes: Math.max(0, totalBytes - freeBytes),
      freeBytes,
    }
  }

  private static readFileOrNull(path: string): string | null {
    try {
      return readFileSync(path, 'utf8')
    } catch {
      return null
    }
  }
}
