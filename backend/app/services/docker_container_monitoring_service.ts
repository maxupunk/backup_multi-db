import { spawn } from 'node:child_process'
import { DockerEngineHttpClient } from '#services/docker_engine_http_client'

type CommandResult = {
  success: boolean
  stdout: string
  stderr: string
}

type DockerContainerListItem = {
  Id?: string
  Names?: string[]
  Image?: string
  State?: string
  Status?: string
}

type DockerContainerStatsItem = {
  cpu_stats?: {
    cpu_usage?: {
      total_usage?: number
      percpu_usage?: number[]
    }
    system_cpu_usage?: number
    online_cpus?: number
  }
  precpu_stats?: {
    cpu_usage?: {
      total_usage?: number
    }
    system_cpu_usage?: number
  }
  memory_stats?: {
    usage?: number
    limit?: number
  }
  networks?: Record<string, { rx_bytes?: number; tx_bytes?: number }>
  blkio_stats?: {
    io_service_bytes_recursive?: Array<{ op?: string; value?: number }>
  }
  pids_stats?: {
    current?: number
  }
}

type DockerCliPsEntry = {
  ID?: string
  Names?: string
  Image?: string
  State?: string
  Status?: string
}

type DockerCliStatsEntry = {
  ID?: string
  Name?: string
  CPUPerc?: string
  MemUsage?: string
  MemPerc?: string
  NetIO?: string
  BlockIO?: string
  PIDs?: string
}

export type DockerContainerResourceMetrics = {
  containerId: string
  containerName: string
  imageName: string
  status: string
  cpu: {
    usagePercent: number
  }
  memory: {
    usageBytes: number
    limitBytes: number
    usagePercent: number
  }
  network: {
    rxBytes: number
    txBytes: number
  }
  blockIo: {
    readBytes: number
    writeBytes: number
  }
  pids: number | null
}

export type DockerContainerResourceOverview = {
  dockerAvailable: boolean
  unavailableReason: string | null
  collectedAt: string
  containers: DockerContainerResourceMetrics[]
}

export class DockerContainerMonitoringService {
  private readonly dockerHttpClient = new DockerEngineHttpClient()

  async getOverview(): Promise<DockerContainerResourceOverview> {
    const socketAvailable = this.dockerHttpClient.isSocketAvailable()
    let socketError: string | null = null

    if (socketAvailable) {
      try {
        const containers = await this.getContainersUsingEngineSocket()
        return this.buildOverview(true, null, containers)
      } catch (error) {
        socketError =
          error instanceof Error ? error.message : 'Falha ao consultar Docker via socket'
      }
    }

    try {
      const containers = await this.getContainersUsingCli()
      return this.buildOverview(true, null, containers)
    } catch (error) {
      const cliError = error instanceof Error ? error.message : 'Falha ao consultar Docker via CLI'
      const reason = socketError ? `${socketError}; ${cliError}` : cliError
      return this.buildOverview(false, reason, [])
    }
  }

  private buildOverview(
    dockerAvailable: boolean,
    unavailableReason: string | null,
    containers: DockerContainerResourceMetrics[]
  ): DockerContainerResourceOverview {
    return {
      dockerAvailable,
      unavailableReason,
      collectedAt: new Date().toISOString(),
      containers: containers.sort((a, b) => a.containerName.localeCompare(b.containerName)),
    }
  }

  private async getContainersUsingEngineSocket(): Promise<DockerContainerResourceMetrics[]> {
    const containers =
      await this.dockerHttpClient.getJson<DockerContainerListItem[]>('/containers/json')
    const mapped = await Promise.all(
      containers
        .map((container) => container.Id?.trim())
        .filter((id): id is string => Boolean(id))
        .map((containerId) => this.mapContainerFromSocket(containerId, containers))
    )

    return mapped.filter((item): item is DockerContainerResourceMetrics => item !== null)
  }

  private async mapContainerFromSocket(
    containerId: string,
    containers: DockerContainerListItem[]
  ): Promise<DockerContainerResourceMetrics | null> {
    const base = containers.find((container) => container.Id === containerId)
    const stats = await this.dockerHttpClient.getJson<DockerContainerStatsItem>(
      `/containers/${containerId}/stats?stream=false`
    )

    const cpuUsage = this.parseCpuUsageFromSocket(stats)
    const memoryUsageBytes = stats.memory_stats?.usage ?? 0
    const memoryLimitBytes = stats.memory_stats?.limit ?? 0
    const memoryUsagePercent =
      memoryLimitBytes > 0 ? this.toPercent((memoryUsageBytes / memoryLimitBytes) * 100) : 0

    const network = this.parseNetworkFromSocket(stats)
    const blockIo = this.parseBlockIoFromSocket(stats)

    return {
      containerId,
      containerName: this.resolveContainerName(base?.Names, containerId),
      imageName: base?.Image?.trim() || 'N/A',
      status: base?.State?.trim() || base?.Status?.trim() || 'unknown',
      cpu: {
        usagePercent: cpuUsage,
      },
      memory: {
        usageBytes: memoryUsageBytes,
        limitBytes: memoryLimitBytes,
        usagePercent: memoryUsagePercent,
      },
      network,
      blockIo,
      pids: Number.isFinite(stats.pids_stats?.current) ? Number(stats.pids_stats?.current) : null,
    }
  }

  private resolveContainerName(names: string[] | undefined, containerId: string): string {
    const preferred = names?.[0]?.replace(/^\//, '').trim()
    return preferred || containerId.slice(0, 12)
  }

  private parseCpuUsageFromSocket(stats: DockerContainerStatsItem): number {
    const currentTotal = stats.cpu_stats?.cpu_usage?.total_usage ?? 0
    const previousTotal = stats.precpu_stats?.cpu_usage?.total_usage ?? 0
    const currentSystem = stats.cpu_stats?.system_cpu_usage ?? 0
    const previousSystem = stats.precpu_stats?.system_cpu_usage ?? 0
    const cpuDelta = currentTotal - previousTotal
    const systemDelta = currentSystem - previousSystem

    const onlineCpus =
      stats.cpu_stats?.online_cpus ?? stats.cpu_stats?.cpu_usage?.percpu_usage?.length ?? 1

    if (cpuDelta <= 0 || systemDelta <= 0 || onlineCpus <= 0) {
      return 0
    }

    return this.toPercent((cpuDelta / systemDelta) * onlineCpus * 100)
  }

  private parseNetworkFromSocket(stats: DockerContainerStatsItem): {
    rxBytes: number
    txBytes: number
  } {
    const networks = stats.networks ?? {}
    let rxBytes = 0
    let txBytes = 0

    for (const net of Object.values(networks)) {
      rxBytes += net.rx_bytes ?? 0
      txBytes += net.tx_bytes ?? 0
    }

    return { rxBytes, txBytes }
  }

  private parseBlockIoFromSocket(stats: DockerContainerStatsItem): {
    readBytes: number
    writeBytes: number
  } {
    const values = stats.blkio_stats?.io_service_bytes_recursive ?? []
    let readBytes = 0
    let writeBytes = 0

    for (const item of values) {
      const operation = (item.op ?? '').toLowerCase()
      const value = item.value ?? 0

      if (operation === 'read') {
        readBytes += value
      }

      if (operation === 'write') {
        writeBytes += value
      }
    }

    return { readBytes, writeBytes }
  }

  private async getContainersUsingCli(): Promise<DockerContainerResourceMetrics[]> {
    const [psResult, statsResult] = await Promise.all([
      this.runDockerCommand(['ps', '--format', '{{json .}}']),
      this.runDockerCommand(['stats', '--no-stream', '--format', '{{json .}}']),
    ])

    if (!psResult.success) {
      throw new Error(psResult.stderr || 'Falha ao executar docker ps')
    }

    if (!statsResult.success) {
      throw new Error(statsResult.stderr || 'Falha ao executar docker stats')
    }

    const psEntries = this.parseJsonLines<DockerCliPsEntry>(psResult.stdout)
    const statsEntries = this.parseJsonLines<DockerCliStatsEntry>(statsResult.stdout)
    const statsById = new Map(
      statsEntries
        .map((entry) => [entry.ID?.trim() || '', entry] as const)
        .filter(([id]) => Boolean(id))
    )

    return psEntries
      .map((entry) => {
        const containerId = entry.ID?.trim()
        if (!containerId) {
          return null
        }

        const stats = statsById.get(containerId)
        const memUsage = this.parseUsagePair(stats?.MemUsage)
        const netUsage = this.parseUsagePair(stats?.NetIO)
        const blockUsage = this.parseUsagePair(stats?.BlockIO)

        return {
          containerId,
          containerName: entry.Names?.trim() || containerId.slice(0, 12),
          imageName: entry.Image?.trim() || 'N/A',
          status: entry.State?.trim() || entry.Status?.trim() || 'unknown',
          cpu: {
            usagePercent: this.parsePercent(stats?.CPUPerc),
          },
          memory: {
            usageBytes: memUsage.first,
            limitBytes: memUsage.second,
            usagePercent: this.parsePercent(stats?.MemPerc),
          },
          network: {
            rxBytes: netUsage.first,
            txBytes: netUsage.second,
          },
          blockIo: {
            readBytes: blockUsage.first,
            writeBytes: blockUsage.second,
          },
          pids: this.parseInteger(stats?.PIDs),
        } satisfies DockerContainerResourceMetrics
      })
      .filter((container): container is DockerContainerResourceMetrics => container !== null)
  }

  private parseJsonLines<T>(value: string): T[] {
    return value
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
      .flatMap((line) => {
        try {
          return [JSON.parse(line) as T]
        } catch {
          return []
        }
      })
  }

  private parseUsagePair(value: string | undefined): { first: number; second: number } {
    if (!value) {
      return { first: 0, second: 0 }
    }

    const [first = '', second = ''] = value.split('/')

    return {
      first: this.parseByteSize(first.trim()),
      second: this.parseByteSize(second.trim()),
    }
  }

  private parseByteSize(input: string): number {
    const value = input.trim()
    if (!value || value === '--') {
      return 0
    }

    const match = value.match(/^([\d.]+)\s*([a-zA-Z]+)?$/)
    if (!match) {
      return 0
    }

    const numeric = Number(match[1])
    if (!Number.isFinite(numeric)) {
      return 0
    }

    const unit = (match[2] ?? 'B').toLowerCase()
    const multipliers: Record<string, number> = {
      b: 1,
      kb: 1000,
      kib: 1024,
      mb: 1000 ** 2,
      mib: 1024 ** 2,
      gb: 1000 ** 3,
      gib: 1024 ** 3,
      tb: 1000 ** 4,
      tib: 1024 ** 4,
      pb: 1000 ** 5,
      pib: 1024 ** 5,
    }

    return Math.round(numeric * (multipliers[unit] ?? 1))
  }

  private parsePercent(value: string | undefined): number {
    if (!value) {
      return 0
    }

    const numeric = Number(value.replace('%', '').replace(',', '.').trim())
    if (!Number.isFinite(numeric)) {
      return 0
    }

    return this.toPercent(numeric)
  }

  private parseInteger(value: string | undefined): number | null {
    if (!value) {
      return null
    }

    const parsed = Number(value.trim())
    if (!Number.isFinite(parsed)) {
      return null
    }

    return Math.max(0, Math.round(parsed))
  }

  private toPercent(value: number): number {
    return Math.min(100, Math.max(0, Math.round(value * 100) / 100))
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
