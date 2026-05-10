import { spawn } from 'node:child_process'
import type {
  DockerDiagnosticJob,
  DockerDiagnosticRunner,
} from '#services/docker_diagnostics_types'

const MAX_OUTPUT_LINES = 120

export class DockerPingDiagnosticRunner implements DockerDiagnosticRunner {
  readonly tool = 'ping' as const

  async run(job: DockerDiagnosticJob, onUpdate: (job: DockerDiagnosticJob) => void): Promise<void> {
    const count = job.count ?? 4

    await new Promise<void>((resolve, reject) => {
      const args = ['-n', '-c', String(count), '-W', '2', job.target]
      const processRef = spawn('ping', args, {
        stdio: ['ignore', 'pipe', 'pipe'],
      })

      let stderrOutput = ''

      const pushLines = (chunk: Buffer) => {
        const lines = chunk
          .toString()
          .split(/\r?\n/)
          .map((line) => line.trim())
          .filter(Boolean)

        if (lines.length === 0) {
          return
        }

        job.outputLines.push(...lines)
        if (job.outputLines.length > MAX_OUTPUT_LINES) {
          job.outputLines.splice(0, job.outputLines.length - MAX_OUTPUT_LINES)
        }

        const latestLatency = [...lines]
          .reverse()
          .map((line) => line.match(/time=([\d.]+)\s*ms/i))
          .find(Boolean)

        if (latestLatency?.[1]) {
          job.latencyMs = Math.round(Number.parseFloat(latestLatency[1]))
        }

        onUpdate(job)
      }

      processRef.stdout?.on('data', pushLines)
      processRef.stderr?.on('data', (chunk: Buffer) => {
        stderrOutput += chunk.toString()
        pushLines(chunk)
      })

      processRef.on('error', (error) => {
        const reason =
          'code' in error && error.code === 'ENOENT'
            ? 'Comando ping não está disponível no runtime do backend.'
            : `Falha ao iniciar ping: ${error.message}`

        reject(new Error(reason))
      })

      processRef.on('close', (code) => {
        if (code === 0) {
          job.summary = `Ping concluído com ${count} tentativa(s).`
          resolve()
          return
        }

        const reason = stderrOutput.trim() || `ping finalizou com código ${code}`
        reject(new Error(reason))
      })
    })
  }
}
