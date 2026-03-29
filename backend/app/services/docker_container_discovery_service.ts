import { spawn } from 'node:child_process'
import type {
  DockerContainerDescriptor,
  DockerDatabaseTypeHint,
  DockerNetworkAttachment,
  DockerPortBinding,
} from '#services/docker_discovery_types'
import { DockerEngineHttpClient } from '#services/docker_engine_http_client'

interface CommandResult {
  success: boolean
  stdout: string
  stderr: string
}

interface DockerInspectContainer {
  Id?: string
  Name?: string
  Config?: {
    Image?: string
    Labels?: Record<string, string>
  }
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
    Ports?: Record<
      string,
      Array<{
        HostIp?: string
        HostPort?: string
      }> | null
    >
  }
}

export class DockerContainerDiscoveryService {
  private readonly dockerHttpClient = new DockerEngineHttpClient()

  async listEligibleContainers(): Promise<DockerContainerDescriptor[]> {
    if (this.dockerHttpClient.isSocketAvailable()) {
      return this.listContainersUsingEngineSocket()
    }

    const commandResult = await this.runDockerCommand(['ps', '-q'])

    if (!commandResult.success) {
      return []
    }

    const ids = commandResult.stdout
      .split('\n')
      .map((id) => id.trim())
      .filter(Boolean)

    if (ids.length === 0) {
      return []
    }

    const inspectResult = await this.runDockerCommand(['inspect', ...ids])
    if (!inspectResult.success) {
      return []
    }

    try {
      const containers = JSON.parse(inspectResult.stdout) as DockerInspectContainer[]
      return containers
        .map((container) => this.normalizeContainer(container))
        .filter((container): container is DockerContainerDescriptor => container !== null)
    } catch {
      return []
    }
  }

  private async listContainersUsingEngineSocket(): Promise<DockerContainerDescriptor[]> {
    try {
      const containers =
        await this.dockerHttpClient.getJson<Array<{ Id?: string }>>('/containers/json')
      const ids = containers
        .map((container) => container.Id?.trim())
        .filter((id): id is string => Boolean(id))

      if (ids.length === 0) {
        return []
      }

      const inspectedContainers = await Promise.all(
        ids.map((id) =>
          this.dockerHttpClient.getJson<DockerInspectContainer>(`/containers/${id}/json`)
        )
      )

      return inspectedContainers
        .map((container) => this.normalizeContainer(container))
        .filter((container): container is DockerContainerDescriptor => container !== null)
    } catch {
      return []
    }
  }

  private normalizeContainer(container: DockerInspectContainer): DockerContainerDescriptor | null {
    const containerId = container.Id?.trim()
    if (!containerId) {
      return null
    }

    const containerName = container.Name?.replace(/^\//, '').trim() || containerId.slice(0, 12)
    const imageName = container.Config?.Image?.trim() || ''
    const labels = container.Config?.Labels ?? {}
    const networks = this.normalizeNetworks(container.NetworkSettings?.Networks)
    const ports = this.normalizePorts(container.NetworkSettings?.Ports)
    const databaseTypeHint = this.detectDatabaseTypeHint(containerName, imageName, ports)

    if (!databaseTypeHint && !this.hasDatabasePort(ports)) {
      return null
    }

    return {
      containerId,
      containerName,
      imageName,
      labels,
      databaseTypeHint,
      networks,
      ports,
    }
  }

  private normalizeNetworks(
    networksMap:
      | Record<
          string,
          {
            NetworkID?: string
            Aliases?: string[]
            Gateway?: string
            IPAddress?: string
          }
        >
      | undefined
  ): DockerNetworkAttachment[] {
    if (!networksMap) {
      return []
    }

    return Object.entries(networksMap).map(([networkName, value]) => ({
      networkId: value.NetworkID ?? networkName,
      networkName,
      aliases: Array.isArray(value.Aliases) ? value.Aliases.filter(Boolean) : [],
      gateway: value.Gateway ?? null,
      ipAddress: value.IPAddress ?? null,
    }))
  }

  private normalizePorts(
    portsMap:
      | Record<
          string,
          Array<{
            HostIp?: string
            HostPort?: string
          }> | null
        >
      | undefined
  ): DockerPortBinding[] {
    if (!portsMap) {
      return []
    }

    const normalized: DockerPortBinding[] = []

    for (const [key, value] of Object.entries(portsMap)) {
      const [containerPortPart, protocol = 'tcp'] = key.split('/')
      const containerPort = Number(containerPortPart)
      if (!Number.isFinite(containerPort) || containerPort <= 0) {
        continue
      }

      if (Array.isArray(value) && value.length > 0) {
        for (const binding of value) {
          const hostPort = Number(binding.HostPort)
          normalized.push({
            containerPort,
            hostPort: Number.isFinite(hostPort) && hostPort > 0 ? hostPort : null,
            protocol,
          })
        }
      } else {
        normalized.push({
          containerPort,
          hostPort: null,
          protocol,
        })
      }
    }

    return normalized
  }

  private detectDatabaseTypeHint(
    containerName: string,
    imageName: string,
    ports: DockerPortBinding[]
  ): DockerDatabaseTypeHint | null {
    const text = `${containerName} ${imageName}`.toLowerCase()

    if (text.includes('postgres')) {
      return 'postgresql'
    }

    if (text.includes('mariadb')) {
      return 'mariadb'
    }

    if (text.includes('mysql')) {
      return 'mysql'
    }

    if (ports.some((port) => port.containerPort === 5432)) {
      return 'postgresql'
    }

    if (ports.some((port) => port.containerPort === 3306)) {
      return 'mysql'
    }

    return null
  }

  private hasDatabasePort(ports: DockerPortBinding[]): boolean {
    return ports.some((port) => port.containerPort === 5432 || port.containerPort === 3306)
  }

  private runDockerCommand(args: string[], timeoutMs = 4000): Promise<CommandResult> {
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
        })
      }, timeoutMs)

      processRef.stdout.on('data', (chunk: Buffer) => {
        stdout += chunk.toString()
      })

      processRef.stderr.on('data', (chunk: Buffer) => {
        stderr += chunk.toString()
      })

      processRef.on('error', () => {
        if (resolved) {
          return
        }
        resolved = true
        clearTimeout(timeoutRef)
        resolve({
          success: false,
          stdout,
          stderr,
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
        })
      })
    })
  }
}
