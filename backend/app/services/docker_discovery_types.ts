export type DockerDatabaseTypeHint = 'postgresql' | 'mysql' | 'mariadb'

export type HostResolutionSource = 'docker_dns' | 'host_ip' | 'fallback'

export interface DockerNetworkAttachment {
  networkId: string
  networkName: string
  aliases: string[]
  gateway: string | null
  ipAddress: string | null
}

export interface DockerPortBinding {
  containerPort: number
  hostPort: number | null
  protocol: string
}

export interface DockerContainerDescriptor {
  containerId: string
  containerName: string
  imageName: string
  labels: Record<string, string>
  databaseTypeHint: DockerDatabaseTypeHint | null
  networks: DockerNetworkAttachment[]
  ports: DockerPortBinding[]
}

export interface DockerEnvironmentContext {
  dockerAvailable: boolean
  unavailableReason: string | null
  backendContainerId: string | null
  backendNetworkIds: string[]
  dockerHostIp: string
}

export interface DockerPortOption {
  containerPort: number
  hostPort: number
  protocol: string
  display: string
}

export interface DockerHostSuggestion {
  containerId: string
  containerName: string
  databaseTypeHint: DockerDatabaseTypeHint | null
  sameNetwork: boolean
  suggestedHost: string
  hostResolutionSource: HostResolutionSource
  networkNames: string[]
  portOptions: DockerPortOption[]
  hasExternalPort: boolean
  connectivityWarning: string | null
}
