import { test } from '@japa/runner'
import { Readable, Transform } from 'node:stream'
import { pipeline } from 'node:stream/promises'

import { RestoreService } from '#services/restore_service'
import type { RestoreOptions } from '#services/restore_service'

/**
 * Passa os chunks pela cadeia de filtros e devolve a saida como Buffer.
 * Recebe Buffers crus de proposito: e' na fronteira entre chunks que mora o bug
 * de UTF-8 que estes testes protegem.
 */
async function runFilters(
  chunks: Buffer[],
  dbType: string,
  options: Partial<RestoreOptions> = {}
): Promise<Buffer> {
  const service = new RestoreService()
  const stages = (service as any).buildFilterStages(dbType, {
    mode: 'full',
    ...options,
  }) as Transform[]

  const collected: Buffer[] = []
  const sink = new Transform({
    transform(chunk: Buffer, _encoding, callback) {
      collected.push(Buffer.from(chunk))
      callback()
    },
  })

  await pipeline([Readable.from(chunks), ...stages, sink])

  return Buffer.concat(collected)
}

/** Divide um texto UTF-8 exatamente no meio de um caractere multibyte. */
function splitMidCharacter(text: string, splitAtByte: number): Buffer[] {
  const full = Buffer.from(text, 'utf8')
  return [full.subarray(0, splitAtByte), full.subarray(splitAtByte)]
}

test.group('RestoreService - filtros e fronteira de chunk', () => {
  test('nao corrompe caractere multibyte partido entre chunks', async ({ assert }) => {
    const sql = 'CREATE TABLE informação (id int);\nGRANT ALL ON informação TO joão;\n'

    // Byte 26 cai no meio do "ç" de "informação" (0xC3 0xA7).
    const full = Buffer.from(sql, 'utf8')
    const splitAt = full.indexOf(Buffer.from('ç', 'utf8')) + 1
    assert.isAbove(splitAt, 0, 'fixture invalida: caractere multibyte nao encontrado')

    const chunks = [full.subarray(0, splitAt), full.subarray(splitAt)]

    const output = await runFilters(chunks, 'postgresql', { noPrivileges: true })
    const text = output.toString('utf8')

    assert.notInclude(text, '�', 'caractere de substituicao indica corrupcao de UTF-8')
    assert.include(text, 'CREATE TABLE informação (id int);')
    assert.notInclude(text, 'GRANT ALL')
  })

  test('preserva acentuacao com varios cortes no meio de caracteres', async ({ assert }) => {
    const sql = "INSERT INTO ação VALUES ('coração', 'não', 'informação');\n"
    const full = Buffer.from(sql, 'utf8')

    // Corta a cada 3 bytes: garante cortes no meio de sequencias multibyte.
    const chunks: Buffer[] = []
    for (let index = 0; index < full.length; index += 3) {
      chunks.push(full.subarray(index, index + 3))
    }

    const output = await runFilters(chunks, 'postgresql', { noComments: true })

    assert.notInclude(output.toString('utf8'), '�')
    assert.include(output.toString('utf8'), 'coração')
    assert.include(output.toString('utf8'), 'informação')
  })

  test('filtro schema-only mantem DDL e remove dados, sem corromper acentos', async ({
    assert,
  }) => {
    const sql = [
      'CREATE TABLE endereço (id int, descrição text);',
      'COPY endereço (id, descrição) FROM stdin;',
      '1\tsão paulo',
      '\\.',
      "INSERT INTO endereço VALUES (2, 'brasília');",
      'CREATE INDEX idx ON endereço (id);',
      '',
    ].join('\n')

    const chunks = splitMidCharacter(sql, Buffer.from(sql, 'utf8').indexOf(0xc3) + 1)
    const output = await runFilters(chunks, 'postgresql', { mode: 'schema-only' })
    const text = output.toString('utf8')

    assert.notInclude(text, '�')
    assert.include(text, 'CREATE TABLE endereço')
    assert.include(text, 'CREATE INDEX idx ON endereço (id);')
    assert.notInclude(text, 'são paulo')
    assert.notInclude(text, 'brasília')
  })

  test('filtro data-only do mysql mantem INSERTs acentuados', async ({ assert }) => {
    const sql = [
      'CREATE TABLE usuário (id int);',
      "INSERT INTO usuário VALUES (1, 'josé');",
      'LOCK TABLES `usuário` WRITE;',
      'UNLOCK TABLES;',
      '',
    ].join('\n')

    const full = Buffer.from(sql, 'utf8')
    const chunks = [full.subarray(0, full.indexOf(0xc3) + 1), full.subarray(full.indexOf(0xc3) + 1)]

    const output = await runFilters(chunks, 'mysql', { mode: 'data-only' })
    const text = output.toString('utf8')

    assert.notInclude(text, '�')
    assert.include(text, "INSERT INTO usuário VALUES (1, 'josé');")
    assert.notInclude(text, 'CREATE TABLE')
  })

  test('linha sem quebra acima do teto falha com erro explicito', async ({ assert }) => {
    const service = new RestoreService()
    const rules = (service as any).buildLineRules('postgresql', {
      mode: 'full',
      noOwner: true,
    })

    // Teto injetado pequeno: o comportamento e o mesmo do limite real de 64 MB,
    // sem precisar alocar 64 MB no teste.
    const filter = (service as any).createRestoreFilter(rules, 128) as Transform

    const semQuebra = Buffer.from('A'.repeat(512), 'utf8')

    await assert.rejects(
      () => pipeline([Readable.from([semQuebra]), filter, new Transform({ transform() {} })]),
      /excede .* sem quebra de linha/
    )
  })

  test('linha longa dentro do teto passa normalmente', async ({ assert }) => {
    const service = new RestoreService()
    const rules = (service as any).buildLineRules('postgresql', {
      mode: 'full',
      noOwner: true,
    })
    const filter = (service as any).createRestoreFilter(rules, 4096) as Transform

    const linha = `INSERT INTO t VALUES ('${'x'.repeat(1000)}');\n`
    const collected: Buffer[] = []
    const sink = new Transform({
      transform(chunk: Buffer, _encoding, callback) {
        collected.push(Buffer.from(chunk))
        callback()
      },
    })

    await pipeline([Readable.from([Buffer.from(linha, 'utf8')]), filter, sink])

    assert.equal(Buffer.concat(collected).toString('utf8'), linha)
  })

  test('sem filtros ativos o conteudo passa byte a byte', async ({ assert }) => {
    const sql = "SELECT 'ação', 'ção', 'ê';\n-- comentário\n"
    const full = Buffer.from(sql, 'utf8')
    const chunks = [full.subarray(0, 9), full.subarray(9, 15), full.subarray(15)]

    const output = await runFilters(chunks, 'postgresql', {})

    assert.isTrue(output.equals(full), 'saida deveria ser identica a entrada')
  })
})
