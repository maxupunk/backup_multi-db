import { request } from 'node:http'
import { existsSync } from 'node:fs'
import type { IncomingMessage } from 'node:http'

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

  async postJson<T>(path: string, body?: unknown, timeoutMs = 5000): Promise<T> {
    return new Promise((resolve, reject) => {
      const bodyStr = body !== undefined ? JSON.stringify(body) : ''

      const req = request(
        {
          socketPath: this.socketPath,
          method: 'POST',
          path,
          headers: {
            'Content-Type': 'application/json',
            'Content-Length': Buffer.byteLength(bodyStr),
          },
        },
        (res) => {
          let responseBody = ''

          res.on('data', (chunk: Buffer) => {
            responseBody += chunk.toString()
          })

          res.on('end', () => {
            const status = res.statusCode ?? 0
            // 304 = container already in desired state (e.g. start on running container)
            if (status === 304) {
              resolve(null as T)
              return
            }

            if (status < 200 || status >= 300) {
              reject(new Error(`Docker Engine respondeu ${status}: ${responseBody || 'sem corpo'}`))
              return
            }

            if (!responseBody.trim()) {
              resolve(null as T)
              return
            }

            try {
              resolve(JSON.parse(responseBody) as T)
            } catch {
              resolve(null as T)
            }
          })
        }
      )

      req.setTimeout(timeoutMs, () => {
        req.destroy(new Error('Tempo limite ao executar ação no Docker Engine'))
      })

      req.on('error', (error) => {
        reject(error)
      })

      if (bodyStr) {
        req.write(bodyStr)
      }
      req.end()
    })
  }

  async deleteJson<T>(path: string, timeoutMs = 5000): Promise<T> {
    return new Promise((resolve, reject) => {
      const req = request(
        {
          socketPath: this.socketPath,
          method: 'DELETE',
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

            if (!body.trim()) {
              resolve(null as T)
              return
            }

            try {
              resolve(JSON.parse(body) as T)
            } catch {
              resolve(null as T)
            }
          })
        }
      )

      req.setTimeout(timeoutMs, () => {
        req.destroy(new Error('Tempo limite ao executar remoção no Docker Engine'))
      })

      req.on('error', (error) => {
        reject(error)
      })

      req.end()
    })
  }

  async getStream(path: string, timeoutMs = 30000): Promise<IncomingMessage> {
    return new Promise((resolve, reject) => {
      const req = request(
        {
          socketPath: this.socketPath,
          method: 'GET',
          path,
        },
        (res) => {
          const status = res.statusCode ?? 0
          if (status < 200 || status >= 300) {
            reject(new Error(`Docker Engine respondeu ${status} ao abrir stream`))
            return
          }
          resolve(res)
        }
      )

      req.setTimeout(timeoutMs, () => {
        req.destroy(new Error('Tempo limite ao abrir stream do Docker Engine'))
      })

      req.on('error', (error) => {
        reject(error)
      })

      req.end()
    })
  }
}
