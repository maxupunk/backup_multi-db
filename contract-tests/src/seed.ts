/**
 * Seeds compartilhados (tarefa 1.5 do roadmap).
 *
 * Tudo e' semeado **pela propria API HTTP**, nunca por acesso direto ao banco.
 * Isso e' o que mantem a suite executavel contra as duas implementacoes sem
 * uma linha de diferenca: o back-roco nao tem o mesmo schema (decisao D4,
 * "schema novo"), entao qualquer seed via SQL seria escrito duas vezes e as
 * duas versoes divergiriam com o tempo.
 *
 * O preco e' que o seed depende de endpoints funcionando. Contra o back-roco
 * em construcao, ele vai falhar cedo e com a mensagem do endpoint que faltou —
 * o que e' a informacao certa na hora certa.
 */

import { mkdirSync, writeFileSync, readFileSync } from 'node:fs'
import { dirname } from 'node:path'
import { loadConfig, stateFilePath } from './config.ts'
import { describeResponse, httpRequest, type ContractResponse } from './http.ts'
import { TEST_BOOTSTRAP_TOKEN } from './server.ts'

export interface SeededUser {
  email: string
  password: string
  fullName: string
  id: number | null
  /** `null` para o usuario inativo, que por definicao nao consegue logar. */
  token: string | null
}

export interface SeedState {
  runId: string
  target: string
  baseUrl: string
  bootstrapToken: string
  users: {
    admin: SeededUser
    member: SeededUser
    inactive: SeededUser
  }
  connections: {
    mysql: number | null
    postgres: number | null
  }
  storages: {
    /** Destino local que o proprio backend cria no boot. */
    local: number | null
    minio: number | null
  }
  /**
   * Backups nao sao semeados nesta fase: criar um exige um banco de origem
   * vivo (docker-compose.test.yml) e um dump real. Fica para a Fase 2, no lote
   * de backups, onde o custo se paga porque ha' teste consumindo.
   */
  backups: Record<string, never>
}

const PASSWORD = 'contract-pass-123'

function unwrap(response: ContractResponse, expected: number[], what: string): ContractResponse {
  if (!expected.includes(response.status)) {
    throw new Error(
      `Seed falhou em ${what}: esperava ${expected.join(' ou ')}.\n${describeResponse(response)}`
    )
  }
  return response
}

function seedRequest(
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  path: string,
  options: { token?: string | null; json?: unknown; query?: Record<string, string | number> } = {}
) {
  return httpRequest(method, path, { ...options, skipCoverage: true, test: 'seed' })
}

async function registerAdmin(): Promise<SeededUser> {
  const user: SeededUser = {
    email: 'admin@contract.test',
    password: PASSWORD,
    fullName: 'Contract Admin',
    id: null,
    token: null,
  }

  // O primeiro usuario vira admin ativo, mas so' com o bootstrap token — o
  // `INITIAL_ADMIN_BOOTSTRAP_TOKEN` esta' configurado de proposito em
  // `server.ts` para que o seed exercite o caminho de producao, e nao o atalho
  // de desenvolvimento.
  const response = unwrap(
    await seedRequest('POST', '/api/auth/register', {
      json: {
        email: user.email,
        password: user.password,
        fullName: user.fullName,
        bootstrapToken: TEST_BOOTSTRAP_TOKEN,
      },
    }),
    [201],
    'registro do admin'
  )

  const data = (response.body as { data?: { token?: string; user?: { id?: number } } }).data
  if (!data?.token) {
    throw new Error(
      `Seed falhou: o registro do admin nao devolveu token. O usuario provavelmente ` +
        `nao era o primeiro do banco — o banco desta execucao deveria estar vazio.\n` +
        describeResponse(response)
    )
  }

  user.token = data.token
  user.id = data.user?.id ?? null
  return user
}

async function registerPending(email: string, fullName: string): Promise<SeededUser> {
  const response = unwrap(
    await seedRequest('POST', '/api/auth/register', {
      json: { email, password: PASSWORD, fullName },
    }),
    [201],
    `registro de ${email}`
  )

  // Do segundo usuario em diante o cadastro nasce inativo e a resposta traz so'
  // a mensagem — sem token e sem id.
  if ((response.body as { data?: unknown }).data) {
    throw new Error(
      `Seed falhou: ${email} deveria nascer inativo (sem token), mas veio com \`data\`.\n` +
        describeResponse(response)
    )
  }

  return { email, password: PASSWORD, fullName, id: null, token: null }
}

async function findUserId(adminToken: string, email: string): Promise<number> {
  const response = unwrap(
    await seedRequest('GET', '/api/users', { token: adminToken, query: { limit: 100 } }),
    [200],
    'listagem de usuarios'
  )

  const body = response.body as { data?: Array<{ id: number; email: string }> }
  const found = body.data?.find((user) => user.email === email)

  if (!found) {
    throw new Error(`Seed falhou: usuario ${email} nao apareceu em GET /api/users.`)
  }

  return found.id
}

async function activate(adminToken: string, userId: number, email: string): Promise<void> {
  // `PATCH /api/users/:id/status` alterna o status, nao o define. Como o
  // usuario acabou de nascer inativo, uma chamada o ativa — se um dia o
  // endpoint virar "definir", a assercao abaixo pega a mudanca.
  const response = unwrap(
    await seedRequest('PATCH', `/api/users/${userId}/status`, { token: adminToken }),
    [200],
    `ativacao de ${email}`
  )

  const isActive = (response.body as { data?: { isActive?: boolean } }).data?.isActive
  if (isActive !== true) {
    throw new Error(
      `Seed falhou: ${email} continua inativo depois do toggle de status.\n` +
        describeResponse(response)
    )
  }
}

async function login(user: SeededUser): Promise<string> {
  const response = unwrap(
    await seedRequest('POST', '/api/auth/login', {
      json: { email: user.email, password: user.password },
    }),
    [200],
    `login de ${user.email}`
  )

  const token = (response.body as { data?: { token?: string } }).data?.token
  if (!token) {
    throw new Error(`Seed falhou: login de ${user.email} nao devolveu token.`)
  }
  return token
}

async function seedConnection(
  token: string,
  payload: Record<string, unknown>
): Promise<number | null> {
  // Criar a conexao nao abre socket com o banco de destino — so' `POST
  // /api/connections/:id/test` faz isso. Por isso o seed nao depende do
  // docker-compose.test.yml estar de pe'.
  const response = await seedRequest('POST', '/api/connections', { token, json: payload })

  if (response.status !== 201 && response.status !== 200) {
    throw new Error(
      `Seed falhou ao criar a conexao ${String(payload.name)}.\n${describeResponse(response)}`
    )
  }

  return (response.body as { data?: { id?: number } }).data?.id ?? null
}

async function seedStorages(token: string): Promise<SeedState['storages']> {
  const listed = unwrap(
    await seedRequest('GET', '/api/storages', { token }),
    [200],
    'listagem de storages'
  )

  // `GET /api/storages` embrulha o paginador: `{ success, data: { meta, data } }`.
  // O aninhamento e' proposital do controller e faz parte do contrato — nao e'
  // engano de leitura aqui.
  const items = (
    listed.body as { data?: { data?: Array<{ id: number; provider?: string; type?: string }> } }
  ).data?.data
  const local = items?.find((item) => item.provider === 'local' || item.type === 'local')?.id ?? null

  const minioResponse = await seedRequest('POST', '/api/storages', {
    token,
    json: {
      name: 'Contract MinIO',
      provider: 'minio',
      config: {
        bucket: 'backups-primary',
        accessKeyId: 'contract-access-key',
        secretAccessKey: 'contract-secret-key',
        endpoint: 'http://127.0.0.1:19000',
        region: 'us-east-1',
        forcePathStyle: true,
      },
    },
  })

  if (minioResponse.status !== 201 && minioResponse.status !== 200) {
    throw new Error(`Seed falhou ao criar o storage MinIO.\n${describeResponse(minioResponse)}`)
  }

  return {
    local,
    minio: (minioResponse.body as { data?: { id?: number } }).data?.id ?? null,
  }
}

export async function seedAll(baseUrl: string): Promise<SeedState> {
  const config = loadConfig()

  const admin = await registerAdmin()
  const adminToken = admin.token!

  const member = await registerPending('member@contract.test', 'Contract Member')
  member.id = await findUserId(adminToken, member.email)
  await activate(adminToken, member.id, member.email)
  member.token = await login(member)

  const inactive = await registerPending('inactive@contract.test', 'Contract Inactive')
  inactive.id = await findUserId(adminToken, inactive.email)

  const storages = await seedStorages(adminToken)

  const connections = {
    mysql: await seedConnection(adminToken, {
      name: 'Contract MySQL',
      type: 'mysql',
      host: '127.0.0.1',
      port: 13306,
      databases: ['fixture_primary'],
      username: 'root',
      password: 'contract-root-pass',
      storageDestinationId: storages.local,
    }),
    postgres: await seedConnection(adminToken, {
      name: 'Contract Postgres',
      type: 'postgresql',
      host: '127.0.0.1',
      port: 15432,
      databases: ['fixture_primary'],
      username: 'postgres',
      password: 'contract-root-pass',
      storageDestinationId: storages.local,
    }),
  }

  return {
    runId: config.runId,
    target: config.target,
    baseUrl,
    bootstrapToken: TEST_BOOTSTRAP_TOKEN,
    users: { admin, member, inactive },
    connections,
    storages,
    backups: {},
  }
}

export function writeState(state: SeedState): void {
  const file = stateFilePath()
  mkdirSync(dirname(file), { recursive: true })
  writeFileSync(file, JSON.stringify(state, null, 2) + '\n', 'utf8')
}

export function readState(): SeedState {
  const file = stateFilePath()

  try {
    return JSON.parse(readFileSync(file, 'utf8')) as SeedState
  } catch (cause) {
    throw new Error(
      `Nao consegui ler o estado da execucao em ${file}. Ele e' escrito pelo ` +
        `globalSetup; se sumiu, o setup nao rodou ou rodou apontando para outro ` +
        `CONTRACT_RUN_ID.`,
      { cause }
    )
  }
}
