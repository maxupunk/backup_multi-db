import { spawn } from 'node:child_process'
import type {
  DockerDiagnosticJob,
  DockerDiagnosticRunner,
} from '#services/docker_diagnostics_types'

const MAX_OUTPUT_LINES = 200

export class DockerCurlDiagnosticRunner implements DockerDiagnosticRunner {
  readonly tool = 'curl' as const

  async run(job: DockerDiagnosticJob, onUpdate: (job: DockerDiagnosticJob) => void): Promise<void> {
    const timeoutMs = job.timeoutMs ?? 2000
    const timeoutSeconds = Math.max(timeoutMs / 1000, 1).toFixed(3)

    await new Promise<void>((resolve, reject) => {
      const args = [
        '--silent',
        '--show-error',
        '--include',
        '--location',
        '--connect-timeout',
        timeoutSeconds,
        '--max-time',
        timeoutSeconds,
        '--write-out',
        '\n__CURL_HTTP_CODE__:%{http_code}\n',
        job.target,
      ]
      const processRef = spawn('curl', args, {
        stdio: ['ignore', 'pipe', 'pipe'],
      })

      let stdoutBuffer = ''
      let stderrBuffer = ''
      let stderrOutput = ''
      let httpCode: number | null = null

      const pushVisibleLines = (lines: string[]) => {
        const visibleLines = lines.filter(Boolean)

        if (visibleLines.length === 0) {
          return
        }

        job.outputLines.push(...visibleLines)
        if (job.outputLines.length > MAX_OUTPUT_LINES) {
          job.outputLines.splice(0, job.outputLines.length - MAX_OUTPUT_LINES)
        }

        onUpdate(job)
      }

      const consumeLines = (lines: string[]) => {
        const visibleLines: string[] = []

        for (const line of lines) {
          const normalizedLine = line.replace(/\r$/, '')
          const httpCodeMatch = normalizedLine.match(/^__CURL_HTTP_CODE__:(\d{3})$/)

          if (httpCodeMatch?.[1]) {
            const parsedCode = Number.parseInt(httpCodeMatch[1], 10)
            httpCode = Number.isNaN(parsedCode) ? null : parsedCode
            continue
          }

          visibleLines.push(normalizedLine)
        }

        pushVisibleLines(visibleLines)
      }

      const readChunk = (chunk: Buffer, source: 'stdout' | 'stderr') => {
        const previousBuffer = source === 'stdout' ? stdoutBuffer : stderrBuffer
        const content = `${previousBuffer}${chunk.toString()}`
        const lines = content.split(/\n/)
        const remainder = lines.pop() ?? ''

        if (source === 'stdout') {
          stdoutBuffer = remainder
        } else {
          stderrBuffer = remainder
          stderrOutput += chunk.toString()
        }

        consumeLines(lines)
      }

      const flushBuffers = () => {
        if (stdoutBuffer) {
          consumeLines([stdoutBuffer])
          stdoutBuffer = ''
        }

        if (stderrBuffer) {
          consumeLines([stderrBuffer])
          stderrBuffer = ''
        }
      }

      processRef.stdout?.on('data', (chunk: Buffer) => {
        readChunk(chunk, 'stdout')
      })

      processRef.stderr?.on('data', (chunk: Buffer) => {
        readChunk(chunk, 'stderr')
      })

      processRef.on('error', (error) => {
        const reason =
          'code' in error && error.code === 'ENOENT'
            ? 'Comando curl não está disponível no runtime do backend.'
            : `Falha ao iniciar curl: ${error.message}`

        reject(new Error(reason))
      })

      processRef.on('close', (code) => {
        flushBuffers()

        if (code === 0) {
          job.summary =
            httpCode !== null && httpCode > 0
              ? `Curl concluído com HTTP ${httpCode}.`
              : 'Curl concluído.'
          resolve()
          return
        }

        const reason = stderrOutput.trim() || `curl finalizou com código ${code}`
        reject(new Error(reason))
      })
    })
  }
}
