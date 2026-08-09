/**
 * Coordenadas do `docker-compose.test.yml`.
 *
 * Um lugar so'. Espalhar host/porta/senha pelos testes garante que, no dia em
 * que uma porta mudar, metade da suite passe a falhar por motivo nenhum e a
 * outra metade continue verde testando um servico que nao existe mais.
 *
 * As senhas aqui sao do compose de teste, que so' escuta em 127.0.0.1 e vive
 * em tmpfs. Nao sao segredo.
 */

export const MYSQL = {
  type: 'mysql' as const,
  host: '127.0.0.1',
  port: 13306,
  username: 'tester',
  password: 'test_pw',
  rootUsername: 'root',
  rootPassword: 'test_root_pw',
  database: 'app_fixture',
  secondaryDatabase: 'fixture_secondary',
}

export const MARIADB = {
  type: 'mariadb' as const,
  host: '127.0.0.1',
  port: 13307,
  username: 'tester',
  password: 'test_pw',
  rootUsername: 'root',
  rootPassword: 'test_root_pw',
  database: 'app_fixture',
  secondaryDatabase: 'fixture_secondary',
}

export const POSTGRES = {
  type: 'postgresql' as const,
  host: '127.0.0.1',
  port: 15432,
  username: 'tester',
  password: 'test_root_pw',
  rootUsername: 'tester',
  rootPassword: 'test_root_pw',
  database: 'app_fixture',
  secondaryDatabase: 'fixture_secondary',
}

export const MINIO = {
  host: '127.0.0.1',
  port: 19000,
  endpoint: 'http://127.0.0.1:19000',
  accessKeyId: 'testaccesskey',
  secretAccessKey: 'testsecretkey',
  region: 'us-east-1',
  buckets: {
    primary: 'backups-primary',
    secondary: 'backups-secondary',
    archives: 'archives',
  },
}

export const SFTP = {
  host: '127.0.0.1',
  port: 12222,
  username: 'tester',
  password: 'test_pw',
  basePath: '/home/tester/backups',
}

/** Todos os bancos, para os testes que precisam varrer os tres motores. */
export const DATABASES = [MYSQL, MARIADB, POSTGRES]

/** Payload de conexao pronto para `POST /api/connections`. */
export function connectionPayload(
  fixture: typeof MYSQL | typeof MARIADB | typeof POSTGRES,
  overrides: Record<string, unknown> = {}
): Record<string, unknown> {
  return {
    name: `Contract ${fixture.type}`,
    type: fixture.type,
    host: fixture.host,
    port: fixture.port,
    databases: [fixture.database],
    username: fixture.username,
    password: fixture.password,
    ...overrides,
  }
}
