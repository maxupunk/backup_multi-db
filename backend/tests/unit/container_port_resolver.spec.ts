import { test } from '@japa/runner'

import { ContainerPortResolver } from '#services/container_port_resolver'
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

test.group('ContainerPortResolver', () => {
  test('keeps the internal container port available even when Docker also publishes a host port', ({
    assert,
  }) => {
    // Regressao real:
    // - o sistema sugeria um host interno do Docker, como "mysql-db" ou "172.22.0.3"
    // - mas a lista de portas preservava apenas a porta publicada no host, como 11961
    // - isso gerava pares invalidos como "mysql-db:11961", que sempre falham
    //   porque 11961 existe somente no host Docker; dentro da mesma rede o acesso correto e 3306.
    //
    // Este teste existe para garantir que a camada base de resolucao de portas nunca mais
    // elimine a opcao interna do container quando houver publicacao externa.
    const resolver = new ContainerPortResolver()

    const options = resolver.resolve(createMysqlContainer())

    assert.deepEqual(options, [
      {
        containerPort: 3306,
        hostPort: 3306,
        protocol: 'tcp',
        display: '3306/tcp (interna — mesma rede Docker)',
        isExternal: false,
      },
      {
        containerPort: 3306,
        hostPort: 11961,
        protocol: 'tcp',
        display: '11961 (externa) -> 3306/tcp (container)',
        isExternal: true,
      },
    ])
  })
})
