import { request } from 'node:http'
import { existsSync } from 'node:fs'

export class DockerEngineHttpClient {
  private readonly socketPath = '/var/run/docker.sock'

  isSocketAvailable(): boolean {
    return existsSync(this.socketPath)
  }

  async getJson<T>(path: string, timeoutMs = 3000): Promise<T> {
    return new Promise((resolve, reject) => {
      const req = request(
        {
          socketPath: this.socketPath,
          method: 'GET',
          path,
        },
        (res) => {
          let body = ''

          res.on('data', (chunk: Buffer) => {
            body += chunk.toString()
          })

          res.on('end', () => {
            const status = res.statusCode ?? 0
            if (status < 200 || status >= 300) {
              reject(new Error(`Docker Engine respondeu ${status}: ${body || 'sem corpo'}`))
              return
            }

            try {
              resolve(JSON.parse(body) as T)
            } catch {
              reject(new Error('Resposta JSON inválida do Docker Engine'))
            }
          })
        }
      )

      req.setTimeout(timeoutMs, () => {
        req.destroy(new Error('Tempo limite ao consultar Docker Engine'))
      })

      req.on('error', (error) => {
        reject(error)
      })

      req.end()
    })
  }
}
