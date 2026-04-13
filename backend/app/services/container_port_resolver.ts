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

    // Portas publicadas no host (hostPort definido)
    const externalOptions = container.ports
      .filter((port) => port.hostPort !== null)
      .filter(portFilter)
      .map((port) => ({
        containerPort: port.containerPort,
        hostPort: port.hostPort as number,
        protocol: port.protocol,
        display: `${port.hostPort} (externa) -> ${port.containerPort}/${port.protocol} (container)`,
        isExternal: true,
      }))

    // Portas apenas expostas internamente (sem mapeamento de host)
    const exposedOnlyPorts = container.ports.filter((port) => port.hostPort === null)
    const internalOptions = exposedOnlyPorts.filter(portFilter).map((port) => ({
      containerPort: port.containerPort,
      hostPort: port.containerPort, // usa a porta interna do container para conectar na mesma rede
      protocol: port.protocol,
      display: `${port.containerPort}/${port.protocol} (interna — mesma rede Docker)`,
      isExternal: false,
    }))

    const all: DockerPortOption[] = [...externalOptions, ...internalOptions]

    const unique = new Map<string, DockerPortOption>()
    for (const option of all) {
      const key = `${option.isExternal ? 'ext' : 'int'}-${option.hostPort}-${option.containerPort}-${option.protocol}`
      unique.set(key, option)
    }

    return [...unique.values()].sort((a, b) => {
      // Externas primeiro, depois por porta
      if (a.isExternal !== b.isExternal) return a.isExternal ? -1 : 1
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
