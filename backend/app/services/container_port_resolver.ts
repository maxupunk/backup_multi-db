import type {
  DockerContainerDescriptor,
  DockerDatabaseTypeHint,
  DockerPortOption,
} from '#services/docker_discovery_types'

export class ContainerPortResolver {
  resolve(container: DockerContainerDescriptor): DockerPortOption[] {
    const expectedPort = this.resolveExpectedContainerPort(container.databaseTypeHint)

    const options = container.ports
      .filter((port) => port.hostPort !== null)
      .filter((port) => {
        if (!expectedPort) {
          return true
        }
        return port.containerPort === expectedPort
      })
      .map((port) => ({
        containerPort: port.containerPort,
        hostPort: port.hostPort as number,
        protocol: port.protocol,
        display: `${port.hostPort} (externa) -> ${port.containerPort}/${port.protocol} (container)`,
      }))

    const unique = new Map<string, DockerPortOption>()
    for (const option of options) {
      const key = `${option.hostPort}-${option.containerPort}-${option.protocol}`
      unique.set(key, option)
    }

    return [...unique.values()].sort((a, b) => a.hostPort - b.hostPort)
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
