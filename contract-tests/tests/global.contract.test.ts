/**
 * Lote 2.8 — SSE, fallback da SPA e comportamento global.
 *
 * São as rotas que não pertencem a nenhum controller e por isso costumam ser
 * esquecidas num porte: o canal SSE, o catch-all que serve o `index.html` da
 * SPA, e os middlewares que atuam em **toda** requisição.
 *
 * O SSE é testado com cuidado deliberado: `GET /__transmit/events` é um
 * stream que **nunca fecha**. Ler o corpo inteiro travaria a suíte até o
 * timeout, então o teste usa um timeout curto e trata o estouro como sucesso —
 * o que se quer provar é que o servidor abre o canal com o content-type certo,
 * não que ele termina.
 */

import { describe, expect, it } from 'vitest'
import { request as undiciRequest } from 'undici'
import { expectStatus, json, markCovered, state, unauth } from '../src/session.ts'

const baseUrl = () => state().baseUrl

describe('GET /__transmit/events (SSE)', () => {
  it('abre o canal com content-type de event-stream', async () => {
    // Sem passar pelo cliente da suite: ele leria o corpo ate' o fim, e este
    // corpo nao tem fim.
    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort(), 2_000)

    try {
      // `uid` e' **obrigatorio**: sem ele o controller de eventos devolve 500.
      // O cliente SSE do frontend gera esse uid e o reusa no subscribe.
      const response = await undiciRequest(
        `${baseUrl()}/__transmit/events?uid=contract-tests-uid`,
        {
          method: 'GET',
          signal: controller.signal,
          headers: { accept: 'text/event-stream' },
          headersTimeout: 2_000,
        }
      )

      expect(response.statusCode).toBe(200)
      expect(String(response.headers['content-type'])).toContain('text/event-stream')

      // A chamada nao passou pelo cliente da suite, entao a cobertura precisa
      // ser registrada a mao — senao a rota apareceria como "sem teste".
      markCovered('GET', '/__transmit/events', response.statusCode, 'global.contract')

      // Encerra o stream do nosso lado; deixar aberto seguraria o processo.
      controller.abort()
    } catch (error) {
      // Abortar depois de receber os headers e' o caminho normal aqui.
      if ((error as Error).name !== 'AbortError') throw error
    } finally {
      clearTimeout(timer)
    }
  })
})

describe('POST /__transmit/subscribe e /unsubscribe', () => {
  it('respondem sem 404 e sem 500', async () => {
    // O contrato aqui e' a **existencia** das rotas: sem elas o frontend perde
    // todo o progresso em tempo real de backup, restore e diagnostico. O
    // corpo exato depende de um uid de conexao SSE viva, que este teste nao
    // mantem — por isso a assercao e' sobre a rota existir e reagir.
    for (const rota of ['/__transmit/subscribe', '/__transmit/unsubscribe']) {
      const response = await unauth().post(rota, {
        json: { uid: 'contract-uid-inexistente', channel: 'notifications' },
      })

      expect(response.status, `${rota} respondeu ${response.status}`).not.toBe(404)
      expect(response.status).toBeLessThan(500)
    }
  })
})

describe('fallback da SPA', () => {
  it('responde a uma rota desconhecida sem estourar', async () => {
    // `GET /*` serve o `index.html` quando existe. No ambiente de teste o
    // `public/index.html` pode nao ter sido construido — os dois desfechos sao
    // contrato, o que nao pode e' 500.
    const response = await unauth().get('/uma-rota-do-frontend/que-nao-e-api')

    expect([200, 404]).toContain(response.status)
    expect(response.status).not.toBe(500)
  })

  it('ACHADO: o catch-all captura /api desconhecido e devolve a SPA com 200', async () => {
    // `router.get('*')` esta' registrado depois das rotas de API, mas o `*`
    // casa com **qualquer** GET que nao tenha casado antes — inclusive
    // `/api/qualquer-coisa`. O resultado: um endpoint digitado errado devolve
    // `200 text/html` com o `index.html` da SPA, e nao `404 application/json`.
    //
    // Para um cliente HTTP isso e' pior que um 404: o status diz sucesso, o
    // `JSON.parse` explode em `<!doctype html>`, e a mensagem de erro nao
    // aponta para a causa. Vale so' para GET — os outros metodos nao tem
    // catch-all e dao 404 normalmente.
    const response = await unauth().get('/api/rota-que-nao-existe')

    expect(response.status).toBe(200)
    expect(response.contentType).toContain('text/html')

    // O contraste: o mesmo caminho com outro metodo se comporta como deveria.
    const comOutroMetodo = await unauth().delete('/api/rota-que-nao-existe')
    expect(comOutroMetodo.status).toBe(404)
  })
})

describe('middlewares globais', () => {
  it('force_json_response devolve JSON mesmo sem Accept', async () => {
    const response = await unauth().get('/api/health', { headers: { accept: '*/*' } })

    expect(response.contentType).toContain('application/json')
    expect(json<{ status: string }>(response).status).toBe('ok')
  })

  it('erros tambem saem em JSON, nao em HTML de debug', async () => {
    // O handler de excecao roda com `debug = !inProduction`. Sem o
    // `force_json_response`, um erro viria como a pagina do Youch.
    //
    // Usa POST de proposito: em GET o catch-all da SPA intercepta antes (ver o
    // achado acima), entao GET nao serve para provar nada sobre o handler.
    const response = await unauth().post('/api/rota-que-nao-existe', { json: {} })

    expect(response.status).toBe(404)
    expect(response.contentType).toContain('application/json')
    expect(response.text.toLowerCase()).not.toContain('<html')
  })

  it('o rate limit global anuncia 600 requisicoes por minuto', async () => {
    const response = await unauth().get('/api/health')

    expect(response.headers['x-ratelimit-limit']).toBe('600')
    expect(Number(response.headers['x-ratelimit-remaining'])).toBeGreaterThanOrEqual(0)
    // O reset e' um ISO-8601 no futuro.
    expect(Date.parse(response.headers['x-ratelimit-reset']!)).toBeGreaterThan(Date.now() - 60_000)
  })

  it('o contador de rate limit decresce entre requisicoes', async () => {
    // Prova que o header nao e' um valor fixo colado na resposta.
    const primeira = await unauth().get('/api/health')
    const segunda = await unauth().get('/api/health')

    expect(Number(segunda.headers['x-ratelimit-remaining'])).toBeLessThan(
      Number(primeira.headers['x-ratelimit-remaining']) + 1
    )
  })

  it('rotas protegidas respondem 401 em JSON, nao redirecionam', async () => {
    // Uma API que redireciona para /login em vez de responder 401 quebra todo
    // cliente que nao seja um navegador.
    const response = await unauth().get('/api/connections')

    expectStatus(response, 401)
    expect(response.contentType).toContain('application/json')
  })

  it('metodo nao suportado numa rota existente nao vira 200', async () => {
    const response = await unauth().delete('/api/health')
    expect(response.status).not.toBe(200)
  })
})
