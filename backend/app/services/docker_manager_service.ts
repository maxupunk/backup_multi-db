import { DockerEngineHttpClient } from '#services/docker_engine_http_client'
import type {
  DockerActionResult,
  DockerContainerDetail,
  DockerContainerGroup,
  DockerContainerSummary,
  DockerImageDetail,
  DockerImageSummary,
  DockerLogEntry,
  DockerLogsOptions,
  DockerMount,
  DockerNetworkDetail,
  DockerNetworkEndpoint,
  DockerNetworkSummary,
  DockerPruneResult,
  DockerVolumeDetail,
  DockerVolumeSummary,
  RawDockerContainerListItem,
  RawDockerImageInspect,
  RawDockerImageListItem,
  RawDockerInspectContainer,
  RawDockerNetworkItem,
  RawDockerVolumeInspect,
  RawDockerVolumeListResponse,
} from '#services/docker_manager_types'

const DOCKER_LABEL_PROJECT = 'com.docker.compose.project'

export class DockerManagerService {
  private readonly client: DockerEngineHttpClient

  constructor(client: DockerEngineHttpClient = new DockerEngineHttpClient()) {
    this.client = client
  }

  isAvailable(): boolean {
    return this.client.isSocketAvailable()
  }

  // ================================================================
  // Containers
  // ================================================================

  async listContainers(): Promise<DockerContainerGroup[]> {
    const raw = await this.client.getJson<RawDockerContainerListItem[]>('/containers/json?all=true')

    const containers: DockerContainerSummary[] = raw.map((c) => this.#mapContainerSummary(c))

    return this.#groupByProject(containers)
  }

  async inspectContainer(id: string): Promise<DockerContainerDetail> {
    const raw = await this.client.getJson<RawDockerInspectContainer>(
      `/containers/${encodeURIComponent(id)}/json`
    )
    return this.#mapContainerDetail(raw)
  }

  async startContainer(id: string): Promise<DockerActionResult> {
    await this.client.postJson<null>(`/containers/${encodeURIComponent(id)}/start`)
    return { success: true, message: 'Container iniciado com sucesso.' }
  }

  async stopContainer(id: string): Promise<DockerActionResult> {
    await this.client.postJson<null>(`/containers/${encodeURIComponent(id)}/stop`)
    return { success: true, message: 'Container parado com sucesso.' }
  }

  async restartContainer(id: string): Promise<DockerActionResult> {
    await this.client.postJson<null>(`/containers/${encodeURIComponent(id)}/restart`)
    return { success: true, message: 'Container reiniciado com sucesso.' }
  }

  async getContainerLogs(id: string, options: DockerLogsOptions = {}): Promise<DockerLogEntry[]> {
    const params = new URLSearchParams()
    params.set('stdout', '1')
    params.set('stderr', '1')

    if (options.tail !== undefined) {
      params.set('tail', String(options.tail))
    } else {
      params.set('tail', '200')
    }

    if (options.since !== undefined) {
      params.set('since', String(options.since))
    }

    if (options.timestamps) {
      params.set('timestamps', '1')
    }

    const stream = await this.client.getStream(
      `/containers/${encodeURIComponent(id)}/logs?${params.toString()}`
    )

    return new Promise((resolve, reject) => {
      const entries: DockerLogEntry[] = []
      const chunks: Buffer[] = []

      stream.on('data', (chunk: Buffer) => {
        chunks.push(chunk)
      })

      stream.on('end', () => {
        const buffer = Buffer.concat(chunks)
        let offset = 0

        // Docker multiplexed stream format: 8-byte header per frame
        // byte[0]: stream type (1=stdout, 2=stderr)
        // bytes[4-7]: frame size (big-endian uint32)
        while (offset + 8 <= buffer.length) {
          const streamType = buffer[offset]
          const frameSize = buffer.readUInt32BE(offset + 4)
          offset += 8

          if (offset + frameSize > buffer.length) break

          const message = buffer
            .subarray(offset, offset + frameSize)
            .toString('utf8')
            .trimEnd()
          offset += frameSize

          if (!message) continue

          // Parse timestamp if present (RFC3339 format prepended by Docker)
          let timestamp = new Date().toISOString()
          let text = message

          if (options.timestamps) {
            const spaceIdx = message.indexOf(' ')
            if (spaceIdx > 0) {
              timestamp = message.slice(0, spaceIdx)
              text = message.slice(spaceIdx + 1)
            }
          }

          entries.push({
            timestamp,
            stream: streamType === 2 ? 'stderr' : 'stdout',
            message: text,
          })
        }

        resolve(entries)
      })

      stream.on('error', reject)
    })
  }

  // ================================================================
  // Volumes
  // ================================================================

  async listVolumes(): Promise<DockerVolumeSummary[]> {
    const raw = await this.client.getJson<RawDockerVolumeListResponse>('/volumes')
    return (raw.Volumes ?? []).map((v) => ({
      name: v.Name ?? '',
      driver: v.Driver ?? '',
      mountpoint: v.Mountpoint ?? '',
      labels: v.Labels ?? {},
      scope: v.Scope ?? 'local',
      createdAt: v.CreatedAt,
    }))
  }

  async inspectVolume(name: string): Promise<DockerVolumeDetail> {
    const raw = await this.client.getJson<RawDockerVolumeInspect>(
      `/volumes/${encodeURIComponent(name)}`
    )
    return {
      name: raw.Name ?? '',
      driver: raw.Driver ?? '',
      mountpoint: raw.Mountpoint ?? '',
      labels: raw.Labels ?? {},
      scope: raw.Scope ?? 'local',
      createdAt: raw.CreatedAt,
      options: raw.Options ?? {},
      status: raw.Status,
    }
  }

  async removeVolume(name: string, force = false): Promise<DockerActionResult> {
    const query = force ? '?force=true' : ''
    await this.client.deleteJson<null>(`/volumes/${encodeURIComponent(name)}${query}`)
    return { success: true, message: 'Volume removido com sucesso.' }
  }

  // ================================================================
  // Networks
  // ================================================================

  async listNetworks(): Promise<DockerNetworkSummary[]> {
    const raw = await this.client.getJson<RawDockerNetworkItem[]>('/networks')
    return raw.map((n) => this.#mapNetworkSummary(n))
  }

  async inspectNetwork(id: string): Promise<DockerNetworkDetail> {
    const raw = await this.client.getJson<RawDockerNetworkItem>(
      `/networks/${encodeURIComponent(id)}`
    )
    return {
      ...this.#mapNetworkSummary(raw),
      containers: Object.entries(raw.Containers ?? {}).reduce(
        (acc, [containerId, c]) => {
          acc[containerId] = {
            containerId,
            name: c.Name ?? '',
            macAddress: c.MacAddress ?? '',
            ipv4Address: c.IPv4Address ?? '',
            ipv6Address: c.IPv6Address ?? '',
          }
          return acc
        },
        {} as DockerNetworkDetail['containers']
      ),
      options: raw.Options ?? {},
    }
  }

  // ================================================================
  // Images
  // ================================================================

  async listImages(): Promise<DockerImageSummary[]> {
    const raw = await this.client.getJson<RawDockerImageListItem[]>('/images/json')
    return raw.map((img) => ({
      id: img.Id ?? '',
      parentId: img.ParentId ?? '',
      repoTags: img.RepoTags ?? [],
      repoDigests: img.RepoDigests ?? [],
      created: img.Created ?? 0,
      size: img.Size ?? 0,
      sharedSize: img.SharedSize ?? 0,
      labels: img.Labels ?? {},
      containers: img.Containers ?? 0,
    }))
  }

  async inspectImage(id: string): Promise<DockerImageDetail> {
    const raw = await this.client.getJson<RawDockerImageInspect>(
      `/images/${encodeURIComponent(id)}/json`
    )
    return {
      id: raw.Id ?? '',
      repoTags: raw.RepoTags ?? [],
      created: raw.Created ?? '',
      size: raw.Size ?? 0,
      config: {
        env: raw.Config?.Env ?? null,
        cmd: raw.Config?.Cmd ?? null,
        entrypoint: raw.Config?.Entrypoint ?? null,
        labels: raw.Config?.Labels ?? {},
        workingDir: raw.Config?.WorkingDir ?? '',
        user: raw.Config?.User ?? '',
      },
      rootFs: {
        type: raw.RootFS?.Type ?? '',
        layers: raw.RootFS?.Layers ?? [],
      },
    }
  }

  async removeImage(id: string, force = false): Promise<DockerActionResult> {
    const query = force ? '?force=true' : ''
    await this.client.deleteJson<null>(`/images/${encodeURIComponent(id)}${query}`)
    return { success: true, message: 'Imagem removida com sucesso.' }
  }

  async pruneImages(): Promise<DockerPruneResult> {
    const raw = await this.client.postJson<{
      ImagesDeleted?: Array<{ Untagged?: string; Deleted?: string }>
      SpaceReclaimed?: number
    }>('/images/prune')
    return {
      imagesDeleted: (raw?.ImagesDeleted ?? []).map((i) => ({
        untagged: i.Untagged,
        deleted: i.Deleted,
      })),
      spaceReclaimed: raw?.SpaceReclaimed ?? 0,
    }
  }

  // ================================================================
  // Helpers privados
  // ================================================================

  #mapContainerSummary(c: RawDockerContainerListItem): DockerContainerSummary {
    return {
      id: c.Id ?? '',
      names: (c.Names ?? []).map((n) => n.replace(/^\//, '')),
      image: c.Image ?? '',
      imageId: c.ImageID ?? '',
      state: (c.State ?? 'unknown') as DockerContainerSummary['state'],
      status: c.Status ?? '',
      labels: c.Labels ?? {},
      ports: c.Ports ?? [],
      created: c.Created ?? 0,
    }
  }

  #mapContainerDetail(raw: RawDockerInspectContainer): DockerContainerDetail {
    const networks: DockerNetworkEndpoint[] = Object.entries(
      raw.NetworkSettings?.Networks ?? {}
    ).map(([networkName, net]) => ({
      networkId: net.NetworkID ?? '',
      networkName,
      ipAddress: net.IPAddress ?? '',
      gateway: net.Gateway ?? '',
      aliases: net.Aliases ?? null,
    }))

    const mounts: DockerMount[] = (raw.Mounts ?? []).map((m) => ({
      type: m.Type ?? 'bind',
      name: m.Name,
      source: m.Source ?? '',
      destination: m.Destination ?? '',
      mode: m.Mode ?? '',
      rw: m.RW ?? false,
    }))

    return {
      id: raw.Id ?? '',
      name: (raw.Name ?? '').replace(/^\//, ''),
      image: raw.Config?.Labels?.['com.docker.compose.service'] ?? '',
      imageId: raw.Image ?? '',
      created: raw.Created ?? '',
      state: {
        status: raw.State?.Status ?? '',
        running: raw.State?.Running ?? false,
        paused: raw.State?.Paused ?? false,
        restarting: raw.State?.Restarting ?? false,
        pid: raw.State?.Pid ?? 0,
        startedAt: raw.State?.StartedAt ?? '',
        finishedAt: raw.State?.FinishedAt ?? '',
        exitCode: raw.State?.ExitCode ?? 0,
      },
      config: {
        hostname: raw.Config?.Hostname ?? '',
        env: raw.Config?.Env ?? [],
        cmd: raw.Config?.Cmd ?? null,
        entrypoint: raw.Config?.Entrypoint ?? null,
        labels: raw.Config?.Labels ?? {},
        workingDir: raw.Config?.WorkingDir ?? '',
        user: raw.Config?.User ?? '',
      },
      hostConfig: {
        restartPolicy: {
          name: raw.HostConfig?.RestartPolicy?.Name ?? 'no',
          maximumRetryCount: raw.HostConfig?.RestartPolicy?.MaximumRetryCount ?? 0,
        },
        networkMode: raw.HostConfig?.NetworkMode ?? '',
      },
      mounts,
      networks,
    }
  }

  #mapNetworkSummary(n: RawDockerNetworkItem): DockerNetworkSummary {
    return {
      id: n.Id ?? '',
      name: n.Name ?? '',
      driver: n.Driver ?? '',
      scope: n.Scope ?? 'local',
      ipam: {
        driver: n.IPAM?.Driver ?? 'default',
        config: (n.IPAM?.Config ?? []).map((c) => ({
          subnet: c.Subnet,
          gateway: c.Gateway,
        })),
      },
      internal: n.Internal ?? false,
      labels: n.Labels ?? {},
      created: n.Created ?? '',
    }
  }

  #groupByProject(containers: DockerContainerSummary[]): DockerContainerGroup[] {
    const groups = new Map<string, DockerContainerSummary[]>()

    for (const container of containers) {
      const project = container.labels[DOCKER_LABEL_PROJECT] ?? '_standalone'
      const existing = groups.get(project) ?? []
      existing.push(container)
      groups.set(project, existing)
    }

    return Array.from(groups.entries()).map(([projectName, items]) => ({
      projectName,
      containers: items,
    }))
  }
}
