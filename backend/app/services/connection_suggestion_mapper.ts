import type {
  DockerContainerDescriptor,
  DockerHostSuggestion,
} from '#services/docker_discovery_types'
import { type ContainerPortResolver } from '#services/container_port_resolver'
import { type NetworkReachabilityResolver } from '#services/network_reachability_resolver'

export interface MapperContext {
  backendNetworkIds: string[]
  dockerHostIp: string
}

export class ConnectionSuggestionMapper {
  constructor(
    private readonly networkResolver: NetworkReachabilityResolver,
    private readonly portResolver: ContainerPortResolver
  ) {}

  map(containers: DockerContainerDescriptor[], context: MapperContext): DockerHostSuggestion[] {
    return containers
      .map((container) => {
        const hostResolution = this.networkResolver.resolve(
          container,
          context.backendNetworkIds,
          context.dockerHostIp
        )
        const allPortOptions = this.portResolver.resolve(container)
        // Quando não está na mesma rede, portas internas são inacessíveis — filtrá-las
        const portOptions = hostResolution.sameNetwork
          ? allPortOptions
          : allPortOptions.filter((opt) => opt.isExternal)
        const hasExternalPort = allPortOptions.some((opt) => opt.isExternal)
        const connectivityWarning =
          !hostResolution.sameNetwork && !hasExternalPort
            ? 'Container selecionado não publica porta externa e não está na mesma rede do sistema. Pode não haver acesso ao banco.'
            : null

        return {
          containerId: container.containerId,
          containerName: container.containerName,
          databaseTypeHint: container.databaseTypeHint,
          sameNetwork: hostResolution.sameNetwork,
          suggestedHost: hostResolution.suggestedHost,
          hostResolutionSource: hostResolution.hostResolutionSource,
          networkNames: container.networks.map((network) => network.networkName),
          portOptions,
          hasExternalPort,
          connectivityWarning,
        }
      })
      .sort((a, b) => a.containerName.localeCompare(b.containerName))
  }
}
