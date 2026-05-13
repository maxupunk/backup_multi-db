import type { DockerPortOption } from '#services/docker_discovery_types'

export interface ResolvedPortSelection {
  portOptions: DockerPortOption[]
  recommendedPort: number | null
}

export class ConnectionPortSelectionResolver {
  resolve(allPortOptions: DockerPortOption[], sameNetwork: boolean): ResolvedPortSelection {
    const accessibleOptions = sameNetwork
      ? allPortOptions
      : allPortOptions.filter((option) => option.isExternal)

    const orderedOptions = [...accessibleOptions].sort((left, right) => {
      const leftPriority = this.resolvePriority(left, sameNetwork)
      const rightPriority = this.resolvePriority(right, sameNetwork)

      if (leftPriority !== rightPriority) {
        return leftPriority - rightPriority
      }

      if (left.hostPort !== right.hostPort) {
        return left.hostPort - right.hostPort
      }

      return left.containerPort - right.containerPort
    })

    return {
      portOptions: orderedOptions,
      recommendedPort: orderedOptions[0]?.hostPort ?? null,
    }
  }

  private resolvePriority(option: DockerPortOption, sameNetwork: boolean): number {
    if (sameNetwork) {
      return option.isExternal ? 1 : 0
    }

    return option.isExternal ? 0 : 1
  }
}
