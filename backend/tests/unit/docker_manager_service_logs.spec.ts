import { Readable } from 'node:stream'
import { test } from '@japa/runner'
import { type DockerEngineHttpClient } from '#services/docker_engine_http_client'
import { DockerManagerService } from '#services/docker_manager_service'

function createLogFrame(streamType: 1 | 2, message: string): Buffer {
  const payload = Buffer.from(message, 'utf8')
  const header = Buffer.alloc(8)

  header[0] = streamType
  header.writeUInt32BE(payload.length, 4)

  return Buffer.concat([header, payload])
}

test.group('DockerManagerService | logs', () => {
  test('inclui since e until na consulta de logs do Docker', async ({ assert }) => {
    let requestedPath = ''

    const fakeClient = {
      getStream: async (path: string) => {
        requestedPath = path

        return Readable.from([
          Buffer.concat([
            createLogFrame(1, '2026-05-16T10:00:00.000000000Z worker ready\n'),
            createLogFrame(2, '2026-05-16T10:00:01.000000000Z worker failed\n'),
          ]),
        ])
      },
    } as DockerEngineHttpClient

    const service = new DockerManagerService(fakeClient)

    const entries = await service.getContainerLogs('abc123', {
      tail: 'all',
      since: 1715853600,
      until: 1715857200,
      timestamps: true,
    })

    assert.include(requestedPath, '/containers/abc123/logs?')
    assert.include(requestedPath, 'stdout=1')
    assert.include(requestedPath, 'stderr=1')
    assert.include(requestedPath, 'tail=all')
    assert.include(requestedPath, 'since=1715853600')
    assert.include(requestedPath, 'until=1715857200')
    assert.include(requestedPath, 'timestamps=1')

    assert.deepEqual(entries, [
      {
        timestamp: '2026-05-16T10:00:00.000000000Z',
        stream: 'stdout',
        message: 'worker ready',
      },
      {
        timestamp: '2026-05-16T10:00:01.000000000Z',
        stream: 'stderr',
        message: 'worker failed',
      },
    ])
  })
})
