import { test } from '@japa/runner'

import { ConnectionPortSelectionResolver } from '#services/connection_port_selection_resolver'
import { ConnectionSuggestionMapper } from '#services/connection_suggestion_mapper'
import { ContainerPortResolver } from '#services/container_port_resolver'
import { NetworkReachabilityResolver } from '#services/network_reachability_resolver'
import type { DockerContainerDescriptor } from '#services/docker_discovery_types'

function createMysqlContainer(): DockerContainerDescriptor {
  return {
    containerId: 'mysql-container-id',
    containerName: 'mysql-db',
    imageName: 'mysql:8.4',
    labels: {},
    databaseTypeHint: 'mysql',
    networks: [
      {
        networkId: 'shared-network',
        networkName: 'shared-network',
        aliases: ['mysql-db'],
        gateway: '172.22.0.1',
        ipAddress: '172.22.0.3',
      },
    ],
    ports: [
      {
        containerPort: 3306,
        hostPort: 11961,
        protocol: 'tcp',
      },
    ],
  }
}

function createMapper(): ConnectionSuggestionMapper {
  return new ConnectionSuggestionMapper(
    new NetworkReachabilityResolver(),
    new ContainerPortResolver(),
    new ConnectionPortSelectionResolver()
  )
}

test.group('ConnectionSuggestionMapper', () => {
  test('recommends the internal container port when the backend is on the same Docker network', ({
    assert,
  }) => {
    // Este e o contrato que protege a regressao reportada pelo usuario.
    // Quando o backend enxerga o banco pela mesma rede Docker, o host sugerido sera um DNS interno
    // ou um IP interno do container. Nesse caso, a porta correta para conectar e a porta interna
    // do proprio container, nao a porta publicada no host.
    //
    // Se esta regra quebrar, a UI volta a montar endpoints invalidos como:
    //   172.22.0.3:11961
    // quando o endpoint correto, na mesma rede, e:
    //   mysql-db:3306
    const mapper = createMapper()

    const suggestions = mapper.map([createMysqlContainer()], {
      backendNetworkIds: ['shared-network'],
      dockerHostIp: 'host.docker.internal',
    })

    assert.lengthOf(suggestions, 1)
    assert.strictEqual(suggestions[0]?.suggestedHost, 'mysql-db')
    assert.strictEqual(suggestions[0]?.recommendedPort, 3306)
    assert.deepEqual(
      suggestions[0]?.portOptions.map((option) => ({
        hostPort: option.hostPort,
        isExternal: option.isExternal,
      })),
      [
        { hostPort: 3306, isExternal: false },
        { hostPort: 11961, isExternal: true },
      ]
    )
  })

  test('recommends only the published host port when the backend is outside the Docker network', ({
    assert,
  }) => {
    // O comportamento oposto tambem precisa ser fixo.
    // Fora da rede Docker, a porta interna do container nao e roteavel, entao a sugestao valida
    // precisa expor apenas a porta publicada no host. Se a opcao interna escapar aqui, o sistema
    // pode sugerir um endpoint invisivel para o backend e a regressao reaparece com outro formato.
    const mapper = createMapper()

    const suggestions = mapper.map([createMysqlContainer()], {
      backendNetworkIds: ['another-network'],
      dockerHostIp: 'host.docker.internal',
    })

    assert.lengthOf(suggestions, 1)
    assert.strictEqual(suggestions[0]?.suggestedHost, 'host.docker.internal')
    assert.strictEqual(suggestions[0]?.recommendedPort, 11961)
    assert.deepEqual(
      suggestions[0]?.portOptions.map((option) => ({
        hostPort: option.hostPort,
        isExternal: option.isExternal,
      })),
      [{ hostPort: 11961, isExternal: true }]
    )
  })
})
