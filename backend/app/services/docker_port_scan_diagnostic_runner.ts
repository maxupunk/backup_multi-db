import net from 'node:net'
import type {
  DockerDiagnosticJob,
  DockerDiagnosticRunner,
} from '#services/docker_diagnostics_types'

export class DockerPortScanDiagnosticRunner implements DockerDiagnosticRunner {
  readonly tool = 'port_scan' as const

  async run(job: DockerDiagnosticJob, onUpdate: (job: DockerDiagnosticJob) => void): Promise<void> {
    if (job.port === null) {
      throw new Error('Porta é obrigatória para o scan')
    }

    const port = job.port
    const timeoutMs = job.timeoutMs ?? 2000
    const startedAt = Date.now()

    job.outputLines.push(`Tentando conexão TCP em ${job.target}:${port}...`)
    onUpdate(job)

    await new Promise<void>((resolve, reject) => {
      const socket = new net.Socket()
      let settled = false

      const finish = (handler: () => void) => {
        if (settled) {
          return
        }

        settled = true
        socket.removeAllListeners()
        socket.destroy()
        handler()
      }

      socket.setTimeout(timeoutMs)

      socket.once('connect', () => {
        finish(() => {
          job.portOpen = true
          job.latencyMs = Date.now() - startedAt
          job.outputLines.push(`Porta ${port} aberta em ${job.target}.`)
          job.summary = `Porta ${port} está em uso e aceitando conexão.`
          onUpdate(job)
          resolve()
        })
      })

      socket.once('timeout', () => {
        finish(() => {
          job.portOpen = false
          job.latencyMs = Date.now() - startedAt
          job.outputLines.push(`Timeout ao conectar em ${job.target}:${port}.`)
          job.summary = `Porta ${port} não respondeu dentro de ${timeoutMs} ms.`
          onUpdate(job)
          resolve()
        })
      })

      socket.once('error', (error: NodeJS.ErrnoException) => {
        finish(() => {
          if (error.code === 'ECONNREFUSED') {
            job.portOpen = false
            job.latencyMs = Date.now() - startedAt
            job.outputLines.push(`Conexão recusada em ${job.target}:${port}.`)
            job.summary = `Porta ${port} está fechada ou sem listener ativo.`
            onUpdate(job)
            resolve()
            return
          }

          reject(new Error(error.message))
        })
      })

      socket.connect(port, job.target)
    })
  }
}
