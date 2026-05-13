import type {
  DockerContainerDescriptor,
  DockerDatabaseTypeHint,
  DockerPortOption,
} from '#services/docker_discovery_types'

export class ContainerPortResolver {
  resolve(container: DockerContainerDescriptor): DockerPortOption[] {
    const expectedPort = this.resolveExpectedContainerPort(container.databaseTypeHint)

    const portFilter = (port: { containerPort: number }) => {
      if (!expectedPort) return true
      return port.containerPort === expectedPort
    }

    const filteredPorts = container.ports.filter(portFilter)

    // A porta interna do container sempre existe para trafego na mesma rede Docker,
    // mesmo quando a mesma porta tambem foi publicada externamente no host.
    const internalOptions = filteredPorts.map((port) => ({
      containerPort: port.containerPort,
      hostPort: port.containerPort,
      protocol: port.protocol,
      display: `${port.containerPort}/${port.protocol} (interna — mesma rede Docker)`,
      isExternal: false,
    }))

    // Portas publicadas no host (hostPort definido)
    const externalOptions = filteredPorts
      .filter((port) => port.hostPort !== null)
      .map((port) => ({
        containerPort: port.containerPort,
        hostPort: port.hostPort as number,
        protocol: port.protocol,
        display: `${port.hostPort} (externa) -> ${port.containerPort}/${port.protocol} (container)`,
        isExternal: true,
      }))

    const all: DockerPortOption[] = [...internalOptions, ...externalOptions]

    const unique = new Map<string, DockerPortOption>()
    for (const option of all) {
      const key = `${option.isExternal ? 'ext' : 'int'}-${option.hostPort}-${option.containerPort}-${option.protocol}`
      unique.set(key, option)
    }

    return [...unique.values()].sort((a, b) => {
      if (a.containerPort !== b.containerPort) {
        return a.containerPort - b.containerPort
      }

      if (a.isExternal !== b.isExternal) {
        return a.isExternal ? 1 : -1
      }

      return a.hostPort - b.hostPort
    })
  }

  private resolveExpectedContainerPort(
    databaseTypeHint: DockerDatabaseTypeHint | null
  ): number | null {
    if (databaseTypeHint === 'postgresql') {
      return 5432
    }

    if (databaseTypeHint === 'mysql' || databaseTypeHint === 'mariadb') {
      return 3306
    }

    return null
  }
}
