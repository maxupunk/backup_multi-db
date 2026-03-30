import os from 'node:os'
import { setTimeout as delay } from 'node:timers/promises'
import { getScheduler } from '#services/scheduler_service'

type CpuSnapshot = {
  idle: number
  total: number
}

export type JobsStatusResponse = {
  isRunning: boolean
  activeJobs: number
  status: 'ok' | 'down'
}

export type CpuResourceMetrics = {
  usagePercent: number
  cores: number
  model: string
}

export type MemoryResourceMetrics = {
  totalBytes: number
  usedBytes: number
  freeBytes: number
  usagePercent: number
}

export type SystemResourceMetrics = {
  cpu: CpuResourceMetrics
  memory: MemoryResourceMetrics
}

export type SystemOverview = {
  version: string
  hostname: string
  platform: string
  architecture: string
  nodeVersion: string
  uptimeSeconds: number
  resources: SystemResourceMetrics
  jobs: JobsStatusResponse
}

export class SystemMonitoringService {
  private static readonly CPU_SAMPLE_INTERVAL_MS = 150
  private static readonly CACHE_TTL_MS = 2_000
  private static readonly VERSION = '1.0.0'

  private static cachedOverview: SystemOverview | null = null
  private static cachedAt = 0

  static async getOverview(): Promise<SystemOverview> {
    const now = Date.now()

    if (this.cachedOverview && now - this.cachedAt < this.CACHE_TTL_MS) {
      return this.cachedOverview
    }

    const overview: SystemOverview = {
      version: this.VERSION,
      hostname: os.hostname(),
      platform: `${os.type()} ${os.release()}`,
      architecture: os.arch(),
      nodeVersion: process.version,
      uptimeSeconds: Math.floor(os.uptime()),
      resources: await this.getResourceMetrics(),
      jobs: this.getJobsStatus(),
    }

    this.cachedOverview = overview
    this.cachedAt = now

    return overview
  }

  private static getJobsStatus(): JobsStatusResponse {
    const schedulerStats = getScheduler().getStats()

    return {
      isRunning: schedulerStats.isRunning,
      activeJobs: schedulerStats.activeJobs,
      status: schedulerStats.isRunning ? 'ok' : 'down',
    }
  }

  static async getResourceMetrics(): Promise<SystemResourceMetrics> {
    const [cpu, memory] = await Promise.all([this.getCpuMetrics(), this.getMemoryMetrics()])

    return {
      cpu,
      memory,
    }
  }

  private static async getCpuMetrics(): Promise<CpuResourceMetrics> {
    const cpuInfo = os.cpus()
    const initialSnapshot = this.captureCpuSnapshot(cpuInfo)

    await delay(this.CPU_SAMPLE_INTERVAL_MS)

    const finalCpuInfo = os.cpus()
    const finalSnapshot = this.captureCpuSnapshot(finalCpuInfo)
    const totalDiff = finalSnapshot.total - initialSnapshot.total
    const idleDiff = finalSnapshot.idle - initialSnapshot.idle
    const rawUsage = totalDiff > 0 ? (1 - idleDiff / totalDiff) * 100 : 0

    return {
      usagePercent: this.roundPercent(rawUsage),
      cores: finalCpuInfo.length,
      model: finalCpuInfo[0]?.model ?? 'N/A',
    }
  }

  private static async getMemoryMetrics(): Promise<MemoryResourceMetrics> {
    const totalBytes = os.totalmem()
    const freeBytes = os.freemem()
    const usedBytes = totalBytes - freeBytes
    const rawUsage = totalBytes > 0 ? (usedBytes / totalBytes) * 100 : 0

    return {
      totalBytes,
      usedBytes,
      freeBytes,
      usagePercent: this.roundPercent(rawUsage),
    }
  }

  private static captureCpuSnapshot(cpuInfo: os.CpuInfo[]): CpuSnapshot {
    return cpuInfo.reduce<CpuSnapshot>(
      (snapshot, cpu) => {
        const times = cpu.times
        const total = times.user + times.nice + times.sys + times.idle + times.irq

        snapshot.idle += times.idle
        snapshot.total += total

        return snapshot
      },
      { idle: 0, total: 0 }
    )
  }

  private static roundPercent(value: number): number {
    return Math.min(100, Math.max(0, Math.round(value * 100) / 100))
  }
}
