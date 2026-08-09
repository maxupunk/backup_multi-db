/**
 * Sessoes e assercoes de resposta (tarefa 1.3 do roadmap).
 *
 * `as('admin')` devolve um cliente ja' autenticado com o token que o seed
 * obteve. Os tokens sao emitidos **uma unica vez por execucao**, no seed, e
 * reaproveitados por todos os testes — nao e' otimizacao, e' necessidade: o
 * limiter de `auth` do backend e' de 5 requisicoes por minuto por IP+email
 * (`app/middleware/rate_limit_middleware.ts`). Uma suite que logasse a cada
 * teste comecaria a tomar 429 no sexto e falharia por motivo nenhum.
 */

import { createClient, describeResponse, type ContractClient, type ContractResponse } from './http.ts'
import { readState, type SeedState } from './seed.ts'

export type Role = 'admin' | 'member' | 'inactive'

let cachedState: SeedState | null = null

export function state(): SeedState {
  cachedState ??= readState()
  return cachedState
}

/** Cliente autenticado como um dos usuarios semeados. */
export function as(role: Role): ContractClient {
  const user = state().users[role]

  if (!user.token) {
    throw new Error(
      role === 'inactive'
        ? `O usuario inativo nao tem token por definicao — ele nao consegue logar. ` +
          `Para testar 401/403 de conta pendente, use \`unauth()\` ou tente o login ` +
          `dentro do proprio teste.`
        : `O usuario \`${role}\` nao tem token no state.json desta execucao.`
    )
  }

  return createClient(user.token)
}

/** Cliente sem `Authorization` — para os casos de 401. */
export function unauth(): ContractClient {
  return createClient(null)
}

/** Cliente com um token sintaticamente valido mas que nao existe no banco. */
export function withBogusToken(): ContractClient {
  return createClient('oat_MQ.Ym9ndXMtc2VjcmV0LXRoYXQtbmV2ZXItZXhpc3RlZA')
}

/**
 * Confere o status e devolve a resposta, para encadear.
 *
 * A mensagem carrega o corpo inteiro: quando um 500 aparece no lugar de um
 * 200, o que resolve o problema e' a stack que veio no corpo, nao o numero.
 */
export function expectStatus(response: ContractResponse, expected: number): ContractResponse {
  if (response.status !== expected) {
    throw new Error(`Esperava HTTP ${expected}.\n${describeResponse(response)}`)
  }
  return response
}

/** Versao para quando mais de um status e' aceitavel. */
export function expectStatusIn(response: ContractResponse, expected: number[]): ContractResponse {
  if (!expected.includes(response.status)) {
    throw new Error(`Esperava HTTP ${expected.join(' ou ')}.\n${describeResponse(response)}`)
  }
  return response
}

/** Corpo JSON tipado, com erro util quando a resposta nao era JSON. */
export function json<T = unknown>(response: ContractResponse): T {
  if (response.body === null || typeof response.body === 'string') {
    throw new Error(`Esperava um corpo JSON.\n${describeResponse(response)}`)
  }
  return response.body as T
}
