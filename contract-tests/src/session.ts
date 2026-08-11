/**
 * Sessoes e assercoes de resposta.
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
import { recordCoverage } from './coverage.ts'
import { matchRoute } from './routes.ts'

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

/**
 * Registra manualmente uma rota exercitada fora do cliente HTTP.
 *
 * Escape hatch com um unico usuario legitimo hoje: `GET /__transmit/events` e'
 * um stream SSE que nunca fecha, entao o teste dele fala direto com o `undici`
 * e aborta a conexao — o cliente da suite leria o corpo inteiro e travaria ate'
 * o timeout. Sem esta chamada, a rota apareceria como "sem teste" tendo sido
 * testada.
 *
 * Use com parcimonia: cada chamada aqui e' cobertura afirmada por decreto, nao
 * observada.
 */
export function markCovered(method: string, path: string, status: number, test: string): void {
  const route = matchRoute(method, path)
  recordCoverage({ key: route?.key ?? null, method, path, status, test })
}

/**
 * O que o ambiente desta execucao consegue testar de verdade.
 *
 * Use com `it.skipIf(!can('mysql'))`. O globalSetup ja' avisou em letras
 * grandes o que esta' faltando, entao o skip aqui nao e' silencioso.
 */
export function can(capability: keyof SeedState['capabilities']): boolean {
  return state().capabilities[capability]
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
