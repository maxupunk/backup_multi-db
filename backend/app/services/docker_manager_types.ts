// ============================================================
// Docker Manager — Tipos TypeScript
// ============================================================

// ------ Container ------

export interface DockerContainerPort {
  IP?: string
  PrivatePort: number
  PublicPort?: number
  Type: string
}

export interface DockerContainerSummary {
  id: string
  names: string[]
  image: string
  imageId: string
  state: 'running' | 'stopped' | 'paused' | 'restarting' | 'dead' | 'created' | 'exited' | string
  status: string
  labels: Record<string, string>
  ports: DockerContainerPort[]
  created: number
}

export interface DockerContainerGroup {
  projectName: string
  containers: DockerContainerSummary[]
}

export interface DockerMount {
  type: string
  name?: string
  source: string
  destination: string
  mode: string
  rw: boolean
}

export interface DockerNetworkEndpoint {
  networkId: string
  networkName: string
  ipAddress: string
  gateway: string
  aliases: string[] | null
}

export interface DockerContainerDetail {
  id: string
  name: string
  image: string
  imageId: string
  created: string
  state: {
    status: string
    running: boolean
    paused: boolean
    restarting: boolean
    pid: number
    startedAt: string
    finishedAt: string
    exitCode: number
  }
  config: {
    hostname: string
    env: string[]
    cmd: string[] | null
    entrypoint: string[] | null
    labels: Record<string, string>
    workingDir: string
    user: string
  }
  hostConfig: {
    restartPolicy: {
      name: string
      maximumRetryCount: number
    }
    networkMode: string
  }
  mounts: DockerMount[]
  networks: DockerNetworkEndpoint[]
}

// ------ Volume ------

export interface DockerVolumeSummary {
  name: string
  driver: string
  mountpoint: string
  labels: Record<string, string>
  scope: string
  createdAt?: string
}

export interface DockerVolumeDetail extends DockerVolumeSummary {
  options: Record<string, string>
  status?: Record<string, unknown>
}

// ------ Network ------

export interface DockerNetworkContainer {
  containerId: string
  name: string
  macAddress: string
  ipv4Address: string
  ipv6Address: string
}

export interface DockerNetworkSummary {
  id: string
  name: string
  driver: string
  scope: string
  ipam: {
    driver: string
    config: Array<{ subnet?: string; gateway?: string }>
  }
  internal: boolean
  labels: Record<string, string>
  created: string
}

export interface DockerNetworkDetail extends DockerNetworkSummary {
  containers: Record<string, DockerNetworkContainer>
  options: Record<string, string>
}

// ------ Image ------

export interface DockerImageSummary {
  id: string
  parentId: string
  repoTags: string[]
  repoDigests: string[]
  created: number
  size: number
  sharedSize: number
  labels: Record<string, string>
  containers: number
}

export interface DockerImageDetail {
  id: string
  repoTags: string[]
  created: string
  size: number
  config: {
    env: string[] | null
    cmd: string[] | null
    entrypoint: string[] | null
    labels: Record<string, string>
    workingDir: string
    user: string
  }
  rootFs: {
    type: string
    layers: string[]
  }
}

// ------ Logs ------

export interface DockerLogEntry {
  timestamp: string
  stream: 'stdout' | 'stderr'
  message: string
}

export interface DockerLogsOptions {
  tail?: number | 'all'
  since?: number
  timestamps?: boolean
  follow?: boolean
}

// ------ Ações ------

export interface DockerActionResult {
  success: boolean
  message: string
}

// ------ Prune ------

export interface DockerPruneResult {
  imagesDeleted: Array<{ untagged?: string; deleted?: string }>
  spaceReclaimed: number
}

// ------ Raw Docker API (tipos internos de parsing) ------

export interface RawDockerContainerListItem {
  Id?: string
  Names?: string[]
  Image?: string
  ImageID?: string
  State?: string
  Status?: string
  Labels?: Record<string, string>
  Ports?: DockerContainerPort[]
  Created?: number
}

export interface RawDockerInspectContainer {
  Id?: string
  Name?: string
  Created?: string
  Image?: string
  Config?: {
    Hostname?: string
    Env?: string[]
    Cmd?: string[] | null
    Entrypoint?: string[] | null
    Labels?: Record<string, string>
    WorkingDir?: string
    User?: string
  }
  State?: {
    Status?: string
    Running?: boolean
    Paused?: boolean
    Restarting?: boolean
    Pid?: number
    StartedAt?: string
    FinishedAt?: string
    ExitCode?: number
  }
  HostConfig?: {
    RestartPolicy?: { Name?: string; MaximumRetryCount?: number }
    NetworkMode?: string
  }
  Mounts?: Array<{
    Type?: string
    Name?: string
    Source?: string
    Destination?: string
    Mode?: string
    RW?: boolean
  }>
  NetworkSettings?: {
    Networks?: Record<
      string,
      {
        NetworkID?: string
        IPAddress?: string
        Gateway?: string
        Aliases?: string[] | null
      }
    >
  }
}

export interface RawDockerVolumeListResponse {
  Volumes?: Array<{
    Name?: string
    Driver?: string
    Mountpoint?: string
    Labels?: Record<string, string>
    Scope?: string
    CreatedAt?: string
  }>
}

export interface RawDockerVolumeInspect {
  Name?: string
  Driver?: string
  Mountpoint?: string
  Labels?: Record<string, string>
  Scope?: string
  CreatedAt?: string
  Options?: Record<string, string>
  Status?: Record<string, unknown>
}

export interface RawDockerNetworkItem {
  Id?: string
  Name?: string
  Driver?: string
  Scope?: string
  IPAM?: {
    Driver?: string
    Config?: Array<{ Subnet?: string; Gateway?: string }>
  }
  Internal?: boolean
  Labels?: Record<string, string>
  Created?: string
  Containers?: Record<
    string,
    {
      Name?: string
      MacAddress?: string
      IPv4Address?: string
      IPv6Address?: string
    }
  >
  Options?: Record<string, string>
}

export interface RawDockerImageListItem {
  Id?: string
  ParentId?: string
  RepoTags?: string[]
  RepoDigests?: string[]
  Created?: number
  Size?: number
  SharedSize?: number
  Labels?: Record<string, string>
  Containers?: number
}

export interface RawDockerImageInspect {
  Id?: string
  RepoTags?: string[]
  Created?: string
  Size?: number
  Config?: {
    Env?: string[] | null
    Cmd?: string[] | null
    Entrypoint?: string[] | null
    Labels?: Record<string, string>
    WorkingDir?: string
    User?: string
  }
  RootFS?: {
    Type?: string
    Layers?: string[]
  }
}
