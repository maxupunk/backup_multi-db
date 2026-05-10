import { randomUUID } from 'node:crypto'
import type {
  DockerDiagnosticJob,
  DockerDiagnosticRunner,
  DockerDiagnosticStartPayload,
  DockerDiagnosticTool,
} from '#services/docker_diagnostics_types'
import { DockerCurlDiagnosticRunner } from '#services/docker_curl_diagnostic_runner'
import { DockerDiagnosticEmitter } from '#services/docker_diagnostic_emitter'
import { DockerPingDiagnosticRunner } from '#services/docker_ping_diagnostic_runner'
import { DockerPortScanDiagnosticRunner } from '#services/docker_port_scan_diagnostic_runner'

const DEFAULT_PING_COUNT = 4
const DEFAULT_TIMEOUT_MS = 2000
const JOB_TTL_MS = 60 * 60 * 1000
const RETENTION_SWEEP_INTERVAL_MS = 5 * 60 * 1000
const MAX_RETAINED_JOBS = 100

export class DockerDiagnosticsService {
  private static jobs = new Map<string, DockerDiagnosticJob>()
  private static cleanupSchedule = new Map<string, number>()
  private static retentionSweepHandle: ReturnType<typeof setInterval> | null = null

  private readonly runners: Map<DockerDiagnosticRunner['tool'], DockerDiagnosticRunner>

  constructor(
    runners: DockerDiagnosticRunner[] = [
      new DockerPingDiagnosticRunner(),
      new DockerPortScanDiagnosticRunner(),
      new DockerCurlDiagnosticRunner(),
    ]
  ) {
    this.runners = new Map(runners.map((runner) => [runner.tool, runner]))
  }

  async start(payload: DockerDiagnosticStartPayload): Promise<DockerDiagnosticJob> {
    await DockerDiagnosticsService.runRetentionSweep()

    const runner = this.runners.get(payload.tool)
    if (!runner) {
      throw new Error(`Ferramenta de diagnóstico não suportada: ${payload.tool}`)
    }

    const job = this.createJob(payload)
    DockerDiagnosticsService.jobs.set(job.id, job)
    DockerDiagnosticEmitter.broadcast(job)

    void this.execute(job, runner)

    return job
  }

  getJob(jobId: string): DockerDiagnosticJob | null {
    return DockerDiagnosticsService.jobs.get(jobId) ?? null
  }

  private createJob(payload: DockerDiagnosticStartPayload): DockerDiagnosticJob {
    const target = this.normalizeTarget(payload.target, payload.tool)

    if (!target) {
      throw new Error('Destino do diagnóstico é obrigatório')
    }

    const port = payload.tool === 'port_scan' ? (payload.port ?? null) : null
    if (payload.tool === 'port_scan' && port === null) {
      throw new Error('Porta é obrigatória para scan de porta')
    }

    return {
      id: `diag-${randomUUID()}`,
      tool: payload.tool,
      status: 'pending',
      target,
      port,
      count: payload.tool === 'ping' ? (payload.count ?? DEFAULT_PING_COUNT) : null,
      timeoutMs: payload.timeoutMs ?? DEFAULT_TIMEOUT_MS,
      startedAt: new Date().toISOString(),
      completedAt: null,
      outputLines: [],
      summary: null,
      error: null,
      portOpen: null,
      latencyMs: null,
    }
  }

  private normalizeTarget(rawTarget: string, tool: DockerDiagnosticTool): string {
    const target = rawTarget.trim()

    if (tool === 'curl') {
      return this.normalizeCurlTarget(target)
    }

    const ipv4WithCidr = target.match(/^(\d{1,3}(?:\.\d{1,3}){3})\/\d{1,2}$/)
    return ipv4WithCidr?.[1] ?? target
  }

  private normalizeCurlTarget(rawTarget: string): string {
    if (!rawTarget) {
      return rawTarget
    }

    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(rawTarget)) {
      return rawTarget
    }

    return `http://${rawTarget}`
  }

  private async execute(job: DockerDiagnosticJob, runner: DockerDiagnosticRunner): Promise<void> {
    try {
      job.status = 'running'
      DockerDiagnosticEmitter.broadcast(job)

      await runner.run(job, (updatedJob) => {
        DockerDiagnosticEmitter.broadcast(updatedJob)
      })

      job.status = 'completed'
      job.completedAt = new Date().toISOString()
      this.scheduleCleanup(job.id)
      DockerDiagnosticEmitter.broadcast(job)
    } catch (error) {
      job.status = 'failed'
      job.completedAt = new Date().toISOString()
      job.error = error instanceof Error ? error.message : 'Falha ao executar diagnóstico'
      if (job.summary === null) {
        job.summary = 'Diagnóstico falhou.'
      }
      this.scheduleCleanup(job.id)
      DockerDiagnosticEmitter.broadcast(job)
    }
  }

  private scheduleCleanup(jobId: string): void {
    DockerDiagnosticsService.cleanupSchedule.set(jobId, Date.now() + JOB_TTL_MS)
    DockerDiagnosticsService.ensureRetentionSweep()
  }

  private static ensureRetentionSweep(): void {
    if (this.retentionSweepHandle !== null) {
      return
    }

    this.retentionSweepHandle = setInterval(() => {
      void this.runRetentionSweep()
    }, RETENTION_SWEEP_INTERVAL_MS)
    this.retentionSweepHandle.unref?.()
  }

  private static stopRetentionSweepIfIdle(): void {
    if (this.retentionSweepHandle === null || this.cleanupSchedule.size > 0) {
      return
    }

    clearInterval(this.retentionSweepHandle)
    this.retentionSweepHandle = null
  }

  private static async runRetentionSweep(nowMs = Date.now()): Promise<void> {
    for (const [jobId, cleanupAt] of this.cleanupSchedule.entries()) {
      if (cleanupAt > nowMs) {
        continue
      }

      this.cleanupSchedule.delete(jobId)
      this.jobs.delete(jobId)
    }

    this.pruneOverflowJobs()
    this.stopRetentionSweepIfIdle()
  }

  private static pruneOverflowJobs(): void {
    const overflow = this.jobs.size - MAX_RETAINED_JOBS
    if (overflow <= 0) {
      return
    }

    const removableJobs = [...this.jobs.values()]
      .filter((job) => job.status === 'completed' || job.status === 'failed')
      .sort((left, right) => {
        const leftTime = new Date(left.completedAt ?? left.startedAt).getTime()
        const rightTime = new Date(right.completedAt ?? right.startedAt).getTime()
        return leftTime - rightTime
      })
      .slice(0, overflow)

    for (const job of removableJobs) {
      this.jobs.delete(job.id)
      this.cleanupSchedule.delete(job.id)
    }
  }
}
