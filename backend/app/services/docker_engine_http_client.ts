import { Agent, request } from 'node:http'
import { existsSync } from 'node:fs'
import type { IncomingMessage, RequestOptions } from 'node:http'

export class DockerEngineHttpClient {
  private readonly socketPath = '/var/run/docker.sock'
  private static readonly agent = new Agent({ keepAlive: true, maxSockets: 4 })
  private static readonly SOCKET_AVAILABILITY_TTL_MS = 30_000
  private static socketAvailabilityCheckedAt = 0
  private static socketAvailabilityCache = false

  isSocketAvailable(): boolean {
    const now = Date.now()

    if (
      now - DockerEngineHttpClient.socketAvailabilityCheckedAt <
      DockerEngineHttpClient.SOCKET_AVAILABILITY_TTL_MS
    ) {
      return DockerEngineHttpClient.socketAvailabilityCache
    }

    DockerEngineHttpClient.socketAvailabilityCache = existsSync(this.socketPath)
    DockerEngineHttpClient.socketAvailabilityCheckedAt = now

    return DockerEngineHttpClient.socketAvailabilityCache
  }

  private createRequestOptions(
    method: 'GET' | 'POST' | 'DELETE',
    path: string,
    headers?: Record<string, number | string>
  ): RequestOptions {
    return {
      socketPath: this.socketPath,
      method,
      path,
      headers,
      agent: DockerEngineHttpClient.agent,
    }
  }

  private collectResponseBody(response: IncomingMessage): Promise<string> {
    return new Promise((resolve, reject) => {
      const chunks: Buffer[] = []

      response.on('data', (chunk: Buffer | string) => {
        chunks.push(typeof chunk === 'string' ? Buffer.from(chunk) : chunk)
      })

      response.on('end', () => {
        resolve(Buffer.concat(chunks).toString())
      })

      response.on('error', (error) => {
        reject(error)
      })
    })
  }

  async getJson<T>(path: string, timeoutMs = 3000): Promise<T> {
    return new Promise((resolve, reject) => {
      const req = request(this.createRequestOptions('GET', path), (res) => {
        void this.collectResponseBody(res)
          .then((body) => {
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
          .catch(reject)
      })

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
        this.createRequestOptions('POST', path, {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(bodyStr),
        }),
        (res) => {
          void this.collectResponseBody(res)
            .then((responseBody) => {
              const status = res.statusCode ?? 0
              // 304 = container already in desired state (e.g. start on running container)
              if (status === 304) {
                resolve(null as T)
                return
              }

              if (status < 200 || status >= 300) {
                reject(
                  new Error(`Docker Engine respondeu ${status}: ${responseBody || 'sem corpo'}`)
                )
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
            .catch(reject)
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
      const req = request(this.createRequestOptions('DELETE', path), (res) => {
        void this.collectResponseBody(res)
          .then((body) => {
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
          .catch(reject)
      })

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
      const req = request(this.createRequestOptions('GET', path), (res) => {
        const status = res.statusCode ?? 0
        if (status < 200 || status >= 300) {
          reject(new Error(`Docker Engine respondeu ${status} ao abrir stream`))
          return
        }
        resolve(res)
      })

      req.setTimeout(timeoutMs, () => {
        req.destroy(new Error('Tempo limite ao abrir stream do Docker Engine'))
      })

      req.on('error', (error) => {
        reject(error)
      })

      req.end()
    })
  }

  /**
   * Faz pull de uma imagem Docker consumindo o stream de progresso até o fim.
   * Lança erro se o pull falhar.
   */
  async pullImage(image: string, timeoutMs = 120_000): Promise<void> {
    const [repo, tag = 'latest'] = image.split(':')
    const path = `/images/create?fromImage=${encodeURIComponent(repo)}&tag=${encodeURIComponent(tag)}`

    return new Promise((resolve, reject) => {
      const req = request(this.createRequestOptions('POST', path), (res) => {
        const status = res.statusCode ?? 0

        if (status < 200 || status >= 300) {
          void this.collectResponseBody(res)
            .then((body) => {
              reject(new Error(`Falha ao baixar imagem "${image}": ${body || status}`))
            })
            .catch(reject)
          return
        }

        // Consume newline-delimited JSON progress until stream ends
        res.resume()
        res.on('end', resolve)
        res.on('error', reject)
      })

      req.setTimeout(timeoutMs, () => {
        req.destroy(new Error(`Tempo limite ao baixar imagem "${image}"`))
      })

      req.on('error', reject)
      req.end()
    })
  }
}
