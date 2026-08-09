/**
 * Sondagem do ambiente antes de rodar a suite.
 *
 * Boa parte da API so' pode ser testada de verdade com coisas de fora: os
 * bancos do `docker-compose.test.yml`, o MinIO, o `mysqldump`/`pg_dump` no
 * PATH, o socket do Docker. Numa maquina sem esses recursos, os testes
 * correspondentes nao tem como passar.
 *
 * A saida escolhida foi sondar e **pular alto**: o globalSetup imprime, em
 * letras grandes, o que nao vai ser testado e por que. Pular em silencio seria
 * o pior dos mundos — a suite ficaria verde exatamente onde nao mediu nada, e
 * a cobertura de rotas diria "coberto" para uma rota que ninguem exercitou.
 */

import { spawnSync } from 'node:child_process'
import { connect } from 'node:net'
import { request as undiciRequest } from 'undici'
import { MARIADB, MINIO, MYSQL, POSTGRES, SFTP } from './fixtures.ts'

export interface Capabilities {
  mysql: boolean
  mariadb: boolean
  postgres: boolean
  minio: boolean
  sftp: boolean
  /** `mysqldump` no PATH do processo que roda o backend. */
  mysqldump: boolean
  /** `pg_dump` no PATH. */
  pgdump: boolean
  /** Socket do Docker acessivel. */
  docker: boolean
}

function tcpReachable(host: string, port: number, timeoutMs = 1_500): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = connect({ host, port })
    const finish = (result: boolean) => {
      socket.destroy()
      resolve(result)
    }

    socket.setTimeout(timeoutMs)
    socket.once('connect', () => finish(true))
    socket.once('timeout', () => finish(false))
    socket.once('error', () => finish(false))
  })
}

function binaryOnPath(command: string): boolean {
  // `--version` porque e' o unico argumento que todos os binarios de interesse
  // aceitam sem efeito colateral.
  // Sem `shell: true`: passar argumentos por shell dispara DEP0190 e nao e'
  // necessario — os binarios de interesse sao executaveis reais no PATH.
  const result = spawnSync(command, ['--version'], { stdio: 'ignore' })
  return result.status === 0
}

/**
 * Pergunta ao **proprio backend** se ele enxerga o Docker.
 *
 * Sondar com o CLI `docker` daria a resposta errada: no Windows o CLI fala com
 * o daemon por named pipe e funciona, enquanto o backend procura
 * `/var/run/docker.sock` e nao acha nada. A capacidade que interessa e' a de
 * quem esta' sob teste, nao a da maquina.
 */
async function dockerReachable(baseUrl: string): Promise<boolean> {
  try {
    const response = await undiciRequest(`${baseUrl}/api/docker/status`, {
      method: 'GET',
      headersTimeout: 5_000,
      bodyTimeout: 5_000,
    })
    const body = (await response.body.json()) as { available?: boolean }
    return body.available === true
  } catch {
    return false
  }
}

export async function probeCapabilities(baseUrl: string): Promise<Capabilities> {
  const [mysql, mariadb, postgres, minio, sftp] = await Promise.all([
    tcpReachable(MYSQL.host, MYSQL.port),
    tcpReachable(MARIADB.host, MARIADB.port),
    tcpReachable(POSTGRES.host, POSTGRES.port),
    tcpReachable(MINIO.host, MINIO.port),
    tcpReachable(SFTP.host, SFTP.port),
  ])

  return {
    mysql,
    mariadb,
    postgres,
    minio,
    sftp,
    mysqldump: binaryOnPath('mysqldump'),
    pgdump: binaryOnPath('pg_dump'),
    docker: await dockerReachable(baseUrl),
  }
}

/** Linhas de aviso para o que estiver faltando. Vazio = ambiente completo. */
export function missingCapabilityWarnings(capabilities: Capabilities): string[] {
  const warnings: string[] = []

  const stack: Array<[keyof Capabilities, string, string]> = [
    ['mysql', `MySQL ${MYSQL.host}:${MYSQL.port}`, 'testes de conexão MySQL'],
    ['mariadb', `MariaDB ${MARIADB.host}:${MARIADB.port}`, 'testes de conexão MariaDB'],
    ['postgres', `PostgreSQL ${POSTGRES.host}:${POSTGRES.port}`, 'testes de conexão PostgreSQL'],
    ['minio', `MinIO ${MINIO.host}:${MINIO.port}`, 'testes de storage S3 (browse, copy, archive)'],
    ['sftp', `SFTP ${SFTP.host}:${SFTP.port}`, 'testes de storage SFTP'],
  ]

  for (const [key, what, impact] of stack) {
    if (!capabilities[key]) {
      warnings.push(`${what} inacessível — pulando ${impact}. Suba \`docker compose -f docker-compose.test.yml up -d\`.`)
    }
  }

  if (!capabilities.mysqldump) {
    warnings.push('`mysqldump` não está no PATH — pulando o caminho feliz de backup MySQL/MariaDB.')
  }
  if (!capabilities.pgdump) {
    warnings.push('`pg_dump` não está no PATH — pulando o caminho feliz de backup PostgreSQL.')
  }
  if (!capabilities.docker) {
    warnings.push(
      'O backend não enxerga o Docker (procura `/var/run/docker.sock`) — pulando o caminho feliz ' +
        'das 25 rotas de `/api/docker`. No Windows isso é esperado: o CLI usa named pipe.'
    )
  }

  return warnings
}
