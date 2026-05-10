import logger from '@adonisjs/core/services/logger'
import transmit from '@adonisjs/transmit/services/main'
import { NOTIFICATION_CHANNELS } from '#services/notification_service'
import type { DockerDiagnosticJob } from '#services/docker_diagnostics_types'

export class DockerDiagnosticEmitter {
  static broadcast(job: DockerDiagnosticJob): void {
    try {
      transmit.broadcast(`${NOTIFICATION_CHANNELS.DOCKER_DIAGNOSTICS}/${job.id}`, {
        id: job.id,
        tool: job.tool,
        status: job.status,
        target: job.target,
        port: job.port,
        count: job.count,
        timeoutMs: job.timeoutMs,
        startedAt: job.startedAt,
        completedAt: job.completedAt,
        outputLines: job.outputLines,
        summary: job.summary,
        error: job.error,
        portOpen: job.portOpen,
        latencyMs: job.latencyMs,
      })
    } catch (error) {
      logger.error(`[DockerDiagnostics] Erro ao emitir progresso: ${error}`)
    }
  }
}
