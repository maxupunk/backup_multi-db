/**
 * Fabricas de recursos descartaveis para os testes da Fase 2.
 *
 * Alguns testes precisam **destruir** o que usam: revogar um token, desativar
 * um usuario, apagar uma conexao. Fazer isso com os recursos do seed
 * quebraria todos os testes seguintes, e a ordem entre arquivos do vitest nao
 * e' contrato — depender dela seria trocar um bug determinista por um
 * intermitente.
 *
 * Como no seed, tudo passa pela API HTTP, e as chamadas nao contam na
 * cobertura: uma fabrica nao afirma nada sobre a resposta.
 */

import { httpRequest } from './http.ts'
import { describeResponse, type ContractResponse } from './http.ts'
import { state } from './session.ts'
import { connectionPayload, MYSQL } from './fixtures.ts'

const PASSWORD = 'contract-pass-123'

function must(response: ContractResponse, expected: number[], what: string): ContractResponse {
  if (!expected.includes(response.status)) {
    throw new Error(`Fabrica falhou em ${what}.\n${describeResponse(response)}`)
  }
  return response
}

function call(
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  path: string,
  options: { token?: string | null; json?: unknown; query?: Record<string, string | number> } = {}
) {
  return httpRequest(method, path, { ...options, skipCoverage: true, test: 'factory' })
}

export interface DisposableUser {
  id: number
  email: string
  password: string
  token: string
}

/**
 * Cria um usuario ativo e logado, descartavel.
 *
 * Custo no limiter de `auth` (5/min por IP+e-mail): 1 registro + 1 login no
 * e-mail informado. Use um e-mail por teste e sobra folga.
 */
export async function createActivatedUser(
  email: string,
  fullName = 'Contract Disposable'
): Promise<DisposableUser> {
  const adminToken = state().users.admin.token!

  must(
    await call('POST', '/api/auth/register', { json: { email, password: PASSWORD, fullName } }),
    [201],
    `registro de ${email}`
  )

  const listed = must(
    await call('GET', '/api/users', { token: adminToken, query: { limit: 100 } }),
    [200],
    'listagem de usuarios'
  )

  const found = (listed.body as { data?: Array<{ id: number; email: string }> }).data?.find(
    (user) => user.email === email
  )
  if (!found) throw new Error(`Fabrica falhou: ${email} nao apareceu em GET /api/users.`)

  // O endpoint alterna o status; o usuario nasceu inativo, entao uma chamada
  // o ativa.
  must(
    await call('PATCH', `/api/users/${found.id}/status`, { token: adminToken }),
    [200],
    `ativacao de ${email}`
  )

  const logged = must(
    await call('POST', '/api/auth/login', { json: { email, password: PASSWORD } }),
    [200],
    `login de ${email}`
  )

  const token = (logged.body as { data?: { token?: string } }).data?.token
  if (!token) throw new Error(`Fabrica falhou: login de ${email} nao devolveu token.`)

  return { id: found.id, email, password: PASSWORD, token }
}

/** Cria um usuario inativo (recem-registrado, sem aprovacao). */
export async function createPendingUser(
  email: string,
  fullName = 'Contract Pending'
): Promise<{ id: number; email: string; password: string }> {
  const adminToken = state().users.admin.token!

  must(
    await call('POST', '/api/auth/register', { json: { email, password: PASSWORD, fullName } }),
    [201],
    `registro de ${email}`
  )

  const listed = must(
    await call('GET', '/api/users', { token: adminToken, query: { limit: 100 } }),
    [200],
    'listagem de usuarios'
  )

  const found = (listed.body as { data?: Array<{ id: number; email: string }> }).data?.find(
    (user) => user.email === email
  )
  if (!found) throw new Error(`Fabrica falhou: ${email} nao apareceu em GET /api/users.`)

  return { id: found.id, email, password: PASSWORD }
}

/** Cria uma conexao descartavel e devolve o id. */
export async function createConnection(
  overrides: Record<string, unknown> = {}
): Promise<{ id: number; name: string }> {
  const adminToken = state().users.admin.token!
  const name = (overrides.name as string) ?? 'Contract Descartavel'

  const response = must(
    await call('POST', '/api/connections', {
      token: adminToken,
      json: connectionPayload(MYSQL, { name, ...overrides }),
    }),
    [200, 201],
    `criacao da conexao ${name}`
  )

  const id = (response.body as { data?: { id?: number } }).data?.id
  if (typeof id !== 'number') throw new Error('Fabrica falhou: a conexao criada nao trouxe id.')

  return { id, name }
}
