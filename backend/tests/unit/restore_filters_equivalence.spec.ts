import { test } from '@japa/runner'
import { Readable, Transform } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import { StringDecoder } from 'node:string_decoder'

import { RestoreService } from '#services/restore_service'
import type { RestoreOptions } from '#services/restore_service'

/**
 * Implementação de REFERÊNCIA: a cadeia de Transforms encadeados que existia
 * antes do colapso em uma única passagem (já com StringDecoder).
 *
 * Existe só neste teste, para provar que a nova implementação produz saída
 * byte a byte idêntica em toda a matriz de opções. Se alguém mudar as regras de
 * filtragem no serviço sem atualizar esta referência, o teste quebra — que é
 * exatamente o objetivo num caminho que reescreve o banco do cliente.
 */
function referenceChain(dbType: string, options: RestoreOptions): Transform[] {
  const stages: Transform[] = []

  if (options.mode === 'schema-only') {
    stages.push(dbType === 'postgresql' ? pgSchemaOnly() : mysqlSchemaOnly())
  } else if (options.mode === 'data-only') {
    stages.push(dbType === 'postgresql' ? pgDataOnly() : mysqlDataOnly())
  }

  if (dbType === 'postgresql') {
    if (options.noOwner) stages.push(lineFilter(/^\s*ALTER\s+.*\s+OWNER\s+TO\s+/i))
    if (options.noPrivileges) stages.push(lineFilter(/^\s*(GRANT|REVOKE)\s+/i))
    if (options.noTablespaces) stages.push(lineFilter(/^\s*SET\s+default_tablespace\s*=/i))
    if (options.noComments) stages.push(lineFilter(/^\s*COMMENT\s+ON\s+/i))
  }

  if ((dbType === 'mysql' || dbType === 'mariadb') && options.noCreateDb) {
    stages.push(lineFilter(/^\s*(CREATE\s+DATABASE|USE\s+`)/i))
  }

  return stages
}

function pgSchemaOnly(): Transform {
  let insideCopyBlock = false
  let buffer = ''
  const decoder = new StringDecoder('utf8')

  return new Transform({
    transform(chunk, _encoding, callback) {
      buffer += decoder.write(chunk)
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      const output: string[] = []
      for (const line of lines) {
        if (insideCopyBlock) {
          if (line === '\\.') insideCopyBlock = false
          continue
        }
        if (/^COPY\s+.*\s+FROM\s+stdin/i.test(line)) {
          insideCopyBlock = true
          continue
        }
        if (/^\s*INSERT\s+INTO\s+/i.test(line)) continue
        output.push(line)
      }

      if (output.length > 0) this.push(output.join('\n') + '\n')
      callback()
    },
    flush(callback) {
      buffer += decoder.end()
      if (buffer && !insideCopyBlock && !/^\s*INSERT\s+INTO\s+/i.test(buffer)) {
        this.push(buffer + '\n')
      }
      callback()
    },
  })
}

function mysqlSchemaOnly(): Transform {
  let buffer = ''
  const decoder = new StringDecoder('utf8')

  return new Transform({
    transform(chunk, _encoding, callback) {
      buffer += decoder.write(chunk)
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      const output: string[] = []
      for (const line of lines) {
        if (/^\s*INSERT\s+INTO\s+/i.test(line)) continue
        if (/^\s*LOCK\s+TABLES\s+/i.test(line)) continue
        if (/^\s*UNLOCK\s+TABLES/i.test(line)) continue
        output.push(line)
      }

      if (output.length > 0) this.push(output.join('\n') + '\n')
      callback()
    },
    flush(callback) {
      buffer += decoder.end()
      if (buffer && !/^\s*(INSERT\s+INTO|LOCK\s+TABLES|UNLOCK\s+TABLES)/i.test(buffer)) {
        this.push(buffer + '\n')
      }
      callback()
    },
  })
}

function pgDataOnly(): Transform {
  let insideCopyBlock = false
  let buffer = ''
  const decoder = new StringDecoder('utf8')

  return new Transform({
    transform(chunk, _encoding, callback) {
      buffer += decoder.write(chunk)
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      const output: string[] = []
      for (const line of lines) {
        if (insideCopyBlock) {
          output.push(line)
          if (line === '\\.') insideCopyBlock = false
          continue
        }
        if (/^COPY\s+.*\s+FROM\s+stdin/i.test(line)) {
          insideCopyBlock = true
          output.push(line)
          continue
        }
        if (/^\s*INSERT\s+INTO\s+/i.test(line)) {
          output.push(line)
          continue
        }
        if (/^\s*(SET|BEGIN|COMMIT|ROLLBACK|SELECT\s+pg_catalog\.set_config)/i.test(line)) {
          output.push(line)
          continue
        }
        if (/^\s*--/.test(line) || line.trim() === '') {
          output.push(line)
          continue
        }
        if (/^\s*ALTER\s+TABLE\s+.*\s+(DISABLE|ENABLE)\s+TRIGGER/i.test(line)) {
          output.push(line)
          continue
        }
      }

      if (output.length > 0) this.push(output.join('\n') + '\n')
      callback()
    },
    flush(callback) {
      buffer += decoder.end()
      if (buffer) this.push(buffer + '\n')
      callback()
    },
  })
}

function mysqlDataOnly(): Transform {
  let buffer = ''
  const decoder = new StringDecoder('utf8')

  return new Transform({
    transform(chunk, _encoding, callback) {
      buffer += decoder.write(chunk)
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      const output: string[] = []
      for (const line of lines) {
        if (/^\s*INSERT\s+INTO\s+/i.test(line)) {
          output.push(line)
          continue
        }
        if (/^\s*(LOCK\s+TABLES|UNLOCK\s+TABLES)/i.test(line)) {
          output.push(line)
          continue
        }
        if (/^\s*SET\s+/i.test(line) || /^\s*\/\*!\d+\s+SET\s+/i.test(line)) {
          output.push(line)
          continue
        }
        if (/^\s*--/.test(line) || /^\s*\/\*/.test(line) || line.trim() === '') {
          output.push(line)
          continue
        }
      }

      if (output.length > 0) this.push(output.join('\n') + '\n')
      callback()
    },
    flush(callback) {
      buffer += decoder.end()
      if (buffer) this.push(buffer + '\n')
      callback()
    },
  })
}

function lineFilter(pattern: RegExp): Transform {
  let buffer = ''
  const decoder = new StringDecoder('utf8')

  return new Transform({
    transform(chunk, _encoding, callback) {
      buffer += decoder.write(chunk)
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      const output: string[] = []
      for (const line of lines) {
        if (!pattern.test(line)) output.push(line)
      }

      if (output.length > 0) this.push(output.join('\n') + '\n')
      callback()
    },
    flush(callback) {
      buffer += decoder.end()
      if (buffer && !pattern.test(buffer)) this.push(buffer + '\n')
      callback()
    },
  })
}

async function runStages(chunks: Buffer[], stages: Transform[]): Promise<Buffer> {
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

function chunkify(text: string, size: number): Buffer[] {
  const full = Buffer.from(text, 'utf8')
  const chunks: Buffer[] = []

  for (let index = 0; index < full.length; index += size) {
    chunks.push(full.subarray(index, index + size))
  }

  return chunks.length ? chunks : [Buffer.alloc(0)]
}

const PG_DUMP = [
  '--',
  '-- PostgreSQL database dump',
  '--',
  '',
  'SET statement_timeout = 0;',
  'SET default_tablespace = pg_default;',
  "SELECT pg_catalog.set_config('search_path', '', false);",
  '',
  'CREATE TABLE public.usuário (id integer NOT NULL, descrição text);',
  'ALTER TABLE public.usuário OWNER TO postgres;',
  "COMMENT ON TABLE public.usuário IS 'usuários do sistema';",
  'GRANT SELECT ON TABLE public.usuário TO leitura;',
  'REVOKE ALL ON TABLE public.usuário FROM público;',
  '',
  'COPY public.usuário (id, descrição) FROM stdin;',
  '1\tjoão',
  '2\tmaría',
  '\\.',
  '',
  "INSERT INTO public.usuário VALUES (3, 'josé');",
  'ALTER TABLE public.usuário DISABLE TRIGGER ALL;',
  'CREATE INDEX idx_usuário ON public.usuário (id);',
  'ALTER TABLE public.usuário ENABLE TRIGGER ALL;',
  '',
].join('\n')

const MYSQL_DUMP = [
  '-- MySQL dump 10.13',
  '/*!40101 SET NAMES utf8mb4 */;',
  'CREATE DATABASE `loja`;',
  'USE `loja`;',
  'SET @@SESSION.SQL_LOG_BIN= 0;',
  'CREATE TABLE `endereço` (`id` int, `descrição` varchar(255));',
  'LOCK TABLES `endereço` WRITE;',
  "INSERT INTO `endereço` VALUES (1,'são paulo'),(2,'brasília');",
  'UNLOCK TABLES;',
  '',
].join('\n')

const PG_OPTION_SETS: Array<Partial<RestoreOptions>> = [
  {},
  { noOwner: true },
  { noPrivileges: true },
  { noTablespaces: true },
  { noComments: true },
  { noOwner: true, noPrivileges: true },
  { noOwner: true, noPrivileges: true, noTablespaces: true, noComments: true },
]

const MODES: RestoreOptions['mode'][] = ['full', 'schema-only', 'data-only']
const CHUNK_SIZES = [1, 3, 7, 64, 4096]

test.group('RestoreService - equivalencia com a cadeia encadeada', () => {
  test('postgresql: saida identica em toda a matriz de opcoes e cortes', async ({ assert }) => {
    const service = new RestoreService()

    for (const mode of MODES) {
      for (const optionSet of PG_OPTION_SETS) {
        for (const chunkSize of CHUNK_SIZES) {
          const options = { mode, ...optionSet } as RestoreOptions
          const chunks = chunkify(PG_DUMP, chunkSize)

          const referenceOutput = await runStages(chunks, referenceChain('postgresql', options))
          const actualOutput = await runStages(
            chunks,
            (service as any).buildFilterStages('postgresql', options)
          )

          assert.isTrue(
            actualOutput.equals(referenceOutput),
            `divergencia em mode=${mode} chunk=${chunkSize} opcoes=${JSON.stringify(optionSet)}\n` +
              `esperado:\n${referenceOutput.toString()}\nobtido:\n${actualOutput.toString()}`
          )
        }
      }
    }
  }).timeout(60_000)

  test('mysql: saida identica em toda a matriz de opcoes e cortes', async ({ assert }) => {
    const service = new RestoreService()

    for (const mode of MODES) {
      for (const noCreateDb of [false, true]) {
        for (const chunkSize of CHUNK_SIZES) {
          const options = { mode, noCreateDb } as RestoreOptions
          const chunks = chunkify(MYSQL_DUMP, chunkSize)

          const referenceOutput = await runStages(chunks, referenceChain('mysql', options))
          const actualOutput = await runStages(
            chunks,
            (service as any).buildFilterStages('mysql', options)
          )

          assert.isTrue(
            actualOutput.equals(referenceOutput),
            `divergencia em mode=${mode} chunk=${chunkSize} noCreateDb=${noCreateDb}`
          )
        }
      }
    }
  }).timeout(60_000)

  test('dump sem quebra de linha final mantem o mesmo tratamento', async ({ assert }) => {
    const service = new RestoreService()

    // O resto final sem "\n" era tratado de forma diferente por cada filtro;
    // esta e a borda mais sutil do colapso.
    const trailingCases = [
      { dbType: 'postgresql', text: 'CREATE TABLE a (id int);\nGRANT ALL ON a TO joao;' },
      { dbType: 'postgresql', text: 'CREATE TABLE a (id int);\nINSERT INTO a VALUES (1);' },
      { dbType: 'postgresql', text: 'COPY a (id) FROM stdin;\n1\t2' },
      { dbType: 'mysql', text: 'CREATE TABLE a (id int);\nINSERT INTO a VALUES (1);' },
      { dbType: 'mysql', text: 'CREATE TABLE a (id int);\nUNLOCK TABLES' },
      { dbType: 'mysql', text: 'CREATE TABLE a (id int);\nUSE `outro`' },
    ]

    for (const { dbType, text } of trailingCases) {
      for (const mode of MODES) {
        const options = {
          mode,
          noOwner: true,
          noPrivileges: true,
          noComments: true,
          noTablespaces: true,
          noCreateDb: true,
        } as RestoreOptions

        const chunks = chunkify(text, 5)

        const referenceOutput = await runStages(chunks, referenceChain(dbType, options))
        const actualOutput = await runStages(
          chunks,
          (service as any).buildFilterStages(dbType, options)
        )

        assert.isTrue(
          actualOutput.equals(referenceOutput),
          `divergencia no resto final: db=${dbType} mode=${mode} texto=${JSON.stringify(text)}\n` +
            `esperado:${JSON.stringify(referenceOutput.toString())}\n` +
            `obtido:  ${JSON.stringify(actualOutput.toString())}`
        )
      }
    }
  }).timeout(60_000)

  test('entrada vazia produz saida vazia nos dois caminhos', async ({ assert }) => {
    const service = new RestoreService()
    const options = { mode: 'schema-only', noOwner: true } as RestoreOptions

    const referenceOutput = await runStages(
      [Buffer.alloc(0)],
      referenceChain('postgresql', options)
    )
    const actualOutput = await runStages(
      [Buffer.alloc(0)],
      (service as any).buildFilterStages('postgresql', options)
    )

    assert.equal(referenceOutput.length, 0)
    assert.isTrue(actualOutput.equals(referenceOutput))
  })

  test('modo full sem opcoes nao insere nenhum estagio na cadeia', async ({ assert }) => {
    const service = new RestoreService()
    const stages = (service as any).buildFilterStages('postgresql', {
      mode: 'full',
    }) as Transform[]

    assert.lengthOf(stages, 0, 'sem filtros ativos o dump deve passar direto')
  })

  test('cadeia colapsada usa um unico estagio mesmo com todas as opcoes', async ({ assert }) => {
    const service = new RestoreService()
    const stages = (service as any).buildFilterStages('postgresql', {
      mode: 'schema-only',
      noOwner: true,
      noPrivileges: true,
      noTablespaces: true,
      noComments: true,
    }) as Transform[]

    // Antes eram 5 Transforms encadeados, cada um com seu proprio buffer de linha.
    assert.lengthOf(stages, 1)
  })
})
