import type {
  DockerContainerDescriptor,
  HostResolutionSource,
} from '#services/docker_discovery_types'

export interface ResolvedHost {
  sameNetwork: boolean
  suggestedHost: string
  hostResolutionSource: HostResolutionSource
}

export class NetworkReachabilityResolver {
  resolve(
    container: DockerContainerDescriptor,
    backendNetworkIds: string[],
    dockerHostIp: string
  ): ResolvedHost {
    const containerNetworkIds = container.networks.map((network) => network.networkId)
    const sameNetwork = containerNetworkIds.some((networkId) =>
      backendNetworkIds.includes(networkId)
    )

    if (sameNetwork) {
      const preferredAlias = this.resolvePreferredAlias(container)
      if (preferredAlias) {
        return {
          sameNetwork: true,
          suggestedHost: preferredAlias,
          hostResolutionSource: 'docker_dns',
        }
      }
    }

    if (dockerHostIp) {
      return {
        sameNetwork: false,
        suggestedHost: dockerHostIp,
        hostResolutionSource: 'host_ip',
      }
    }

    return {
      sameNetwork: false,
      suggestedHost: container.containerName,
      hostResolutionSource: 'fallback',
    }
  }

  private resolvePreferredAlias(container: DockerContainerDescriptor): string {
    for (const network of container.networks) {
      const alias = network.aliases.find((candidate) => this.isUsefulAlias(candidate, container))
      if (alias) {
        return alias
      }
    }

    return container.containerName
  }

  private isUsefulAlias(alias: string, container: DockerContainerDescriptor): boolean {
    const value = alias.trim()
    if (!value) {
      return false
    }

    if (value === container.containerId || value === container.containerId.slice(0, 12)) {
      return false
    }

    return true
  }
}
