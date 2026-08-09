import { describe, expect, it } from 'vitest'
import { REDACTED, redact, redactHeaders } from '../../src/redact.ts'

describe('redact', () => {
  it('e idempotente — gravar duas vezes da o mesmo resultado', () => {
    const body = { id: 1, createdAt: '2026-08-09T12:00:00.000Z', name: 'x' }
    const once = redact(body)
    expect(redact(once)).toEqual(once)
  })

  it('estabiliza o que muda a cada execucao', () => {
    expect(
      redact({
        id: 42,
        connectionId: 7,
        createdAt: '2026-08-09T12:00:00.000Z',
        updated_at: '2026-08-09T12:00:00.000Z',
        token: 'oat_MQ.YWJj',
      })
    ).toEqual({
      id: REDACTED.id,
      connectionId: REDACTED.id,
      createdAt: REDACTED.timestamp,
      updated_at: REDACTED.timestamp,
      token: REDACTED.secret,
    })
  })

  it('ordena as chaves para que a ordem de serializacao nao vire diff', () => {
    expect(Object.keys(redact({ zeta: 1, alfa: 2, meio: 3 }) as object)).toEqual([
      'alfa',
      'meio',
      'zeta',
    ])
  })

  it('nao confunde palavras que apenas terminam em id/at', () => {
    // `valid`, `uuid`, `format` e `flat` terminam com as mesmas letras dos
    // padroes de id e timestamp. Redigi-los apagaria contrato de verdade.
    expect(
      redact({ valid: true, format: 'sql', flat: false, paid: 10, grid: 'a' })
    ).toEqual({ valid: true, format: 'sql', flat: false, paid: 10, grid: 'a' })
  })

  it('redige por formato do valor, nao so pelo nome da chave', () => {
    const result = redact({
      mensagem: '2026-08-09T12:00:00.000Z',
      referencia: 'b3f1c2d4-5e6a-4b7c-8d9e-0f1a2b3c4d5e',
      arquivo: '/storage/backups/dump.sql',
    }) as Record<string, string>

    expect(result.mensagem).toBe(REDACTED.timestamp)
    expect(result.referencia).toBe(REDACTED.uuid)
    expect(result.arquivo).toBe(REDACTED.path)
  })

  it('preserva o que keepPaths pedir', () => {
    // `version` do /api/health seria pega por regra nenhuma hoje, mas o
    // mecanismo precisa funcionar para casos como `data.id` de um recurso fixo.
    expect(redact({ data: { id: 3, nome: 'x' } }, { keepPaths: ['data.id'] })).toEqual({
      data: { id: 3, nome: 'x' },
    })
  })

  it('redige caminhos extras informados pelo teste', () => {
    expect(redact({ data: { apelido: 'ana' } }, { extraPaths: ['data.apelido'] })).toEqual({
      data: { apelido: REDACTED.secret },
    })
  })

  it('desce em arrays', () => {
    expect(redact({ itens: [{ id: 1 }, { id: 2 }] })).toEqual({
      itens: [{ id: REDACTED.id }, { id: REDACTED.id }],
    })
  })
})

describe('redactHeaders', () => {
  it('remove cabecalhos volateis e mantem os de contrato', () => {
    const result = redactHeaders({
      'Date': 'Sun, 09 Aug 2026 12:00:00 GMT',
      'Content-Length': '128',
      'X-RateLimit-Remaining': '599',
      'X-RateLimit-Limit': '600',
      'Content-Type': 'application/json; charset=utf-8',
    })

    expect(result).toEqual({
      'x-ratelimit-limit': '600',
      'content-type': 'application/json; charset=utf-8',
    })
  })
})
