import { spawn } from 'node:child_process'
import type {
  DockerEnvironmentContext,
  DockerNetworkAttachment,
} from '#services/docker_discovery_types'
import { DockerEngineHttpClient } from '#services/docker_engine_http_client'

interface CommandResult {
  success: boolean
  stdout: string
  stderr: string
  error: string | null
}

export class DockerEnvironmentService {
  private readonly dockerHttpClient = new DockerEngineHttpClient()

  async getContext(): Promise<DockerEnvironmentContext> {
    const probe = await this.probeDocker()

    if (!probe.success) {
      return {
        dockerAvailable: false,
        unavailableReason: this.resolveUnavailableReason(probe),
        backendContainerId: null,
        backendNetworkIds: [],
        dockerHostIp: this.resolveDockerHostIp([], null),
      }
    }

    const backendContainerId = this.resolveBackendContainerId()
    const backendNetworks = backendContainerId
      ? await this.inspectContainerNetworks(backendContainerId)
      : []

    return {
      dockerAvailable: true,
      unavailableReason: null,
      backendContainerId,
      backendNetworkIds: backendNetworks.map((network) => network.networkId),
      dockerHostIp: this.resolveDockerHostIp(backendNetworks, backendContainerId),
    }
  }

  private resolveBackendContainerId(): string | null {
    const explicitContainerId = process.env.BACKEND_CONTAINER_ID?.trim()
    if (explicitContainerId) {
      return explicitContainerId
    }

    const hostname = process.env.HOSTNAME?.trim()
    if (hostname && /^[a-f0-9]{12,64}$/i.test(hostname)) {
      return hostname
    }

    return null
  }

  private async inspectContainerNetworks(containerId: string): Promise<DockerNetworkAttachment[]> {
    try {
      if (this.dockerHttpClient.isSocketAvailable()) {
        const containerData = await this.dockerHttpClient.getJson<{
          NetworkSettings?: {
            Networks?: Record<
              string,
              {
                NetworkID?: string
                Aliases?: string[]
                Gateway?: string
                IPAddress?: string
              }
            >
          }
        }>(`/containers/${containerId}/json`)
        return this.mapNetworks(containerData)
      }

      const inspect = await this.runDockerCommand(['inspect', containerId])
      if (!inspect.success) {
        return []
      }

      const parsed = JSON.parse(inspect.stdout) as Array<{
        NetworkSettings?: {
          Networks?: Record<
            string,
            {
              NetworkID?: string
              Aliases?: string[]
              Gateway?: string
              IPAddress?: string
            }
          >
        }
      }>
      const containerData = parsed[0] ?? {}
      return this.mapNetworks(containerData)
    } catch {
      return []
    }
  }

  private mapNetworks(containerData: {
    NetworkSettings?: {
      Networks?: Record<
        string,
        {
          NetworkID?: string
          Aliases?: string[]
          Gateway?: string
          IPAddress?: string
        }
      >
    }
  }): DockerNetworkAttachment[] {
    const networks = containerData.NetworkSettings?.Networks ?? {}

    return Object.entries(networks).map(([networkName, value]) => ({
      networkId: value.NetworkID ?? networkName,
      networkName,
      aliases: Array.isArray(value.Aliases) ? value.Aliases.filter(Boolean) : [],
      gateway: value.Gateway ?? null,
      ipAddress: value.IPAddress ?? null,
    }))
  }

  private async probeDocker(): Promise<CommandResult> {
    if (this.dockerHttpClient.isSocketAvailable()) {
      try {
        await this.dockerHttpClient.getJson<{ Version?: string }>('/version')
        return {
          success: true,
          stdout: 'docker-engine-socket',
          stderr: '',
          error: null,
        }
      } catch (error) {
        return {
          success: false,
          stdout: '',
          stderr: '',
          error: error instanceof Error ? error.message : 'Falha ao consultar Docker Engine',
        }
      }
    }

    return this.runDockerCommand(['info', '--format', '{{json .ServerVersion}}'])
  }

  private resolveUnavailableReason(result: CommandResult): string {
    const message = (result.error ?? result.stderr ?? '').trim()
    const socketAvailable = this.dockerHttpClient.isSocketAvailable()

    if (message.includes('ENOENT') && !socketAvailable) {
      return 'Docker CLI não está disponível no container e o socket /var/run/docker.sock não está montado. Monte o socket Docker no serviço backend para habilitar a descoberta.'
    }

    if (message.includes('EACCES')) {
      return 'Sem permissão para acessar o Docker Engine. Garanta permissão de leitura/escrita no socket /var/run/docker.sock para o processo do backend.'
    }

    if (!socketAvailable && !message) {
      return 'Docker indisponível: socket /var/run/docker.sock não encontrado.'
    }

    return message || 'Docker indisponível'
  }

  private resolveDockerHostIp(
    backendNetworks: DockerNetworkAttachment[],
    backendContainerId: string | null
  ): string {
    const envHostIp = process.env.DOCKER_HOST_IP?.trim()
    if (envHostIp) {
      return envHostIp
    }

    const firstGateway = backendNetworks.find((network) => network.gateway)?.gateway
    if (firstGateway) {
      return firstGateway
    }

    if (backendContainerId) {
      return 'host.docker.internal'
    }

    return process.env.DOCKER_FALLBACK_HOST?.trim() || '127.0.0.1'
  }

  private runDockerCommand(args: string[], timeoutMs = 3000): Promise<CommandResult> {
    return new Promise((resolve) => {
      const processRef = spawn('docker', args, {
        stdio: ['ignore', 'pipe', 'pipe'],
      })

      let stdout = ''
      let stderr = ''
      let resolved = false

      const timeoutRef = setTimeout(() => {
        if (resolved) {
          return
        }
        resolved = true
        processRef.kill()
        resolve({
          success: false,
          stdout,
          stderr,
          error: `Tempo limite ao executar: docker ${args.join(' ')}`,
        })
      }, timeoutMs)

      processRef.stdout.on('data', (chunk: Buffer) => {
        stdout += chunk.toString()
      })

      processRef.stderr.on('data', (chunk: Buffer) => {
        stderr += chunk.toString()
      })

      processRef.on('error', (error) => {
        if (resolved) {
          return
        }
        resolved = true
        clearTimeout(timeoutRef)
        resolve({
          success: false,
          stdout,
          stderr,
          error: error.message,
        })
      })

      processRef.on('close', (code) => {
        if (resolved) {
          return
        }
        resolved = true
        clearTimeout(timeoutRef)
        resolve({
          success: code === 0,
          stdout,
          stderr,
          error: code === 0 ? null : stderr.trim() || `Código de saída ${code}`,
        })
      })
    })
  }
}
