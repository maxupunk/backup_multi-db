import { test } from '@japa/runner'
import { createReadStream, readFileSync, writeFileSync } from 'node:fs'
import { unlink } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Readable, type Transform } from 'node:stream'
import { createGunzip, gzipSync } from 'node:zlib'

import { RestoreService } from '#services/restore_service'

type RestoreResult = {
  success: boolean
  error?: string
  exitCode?: number
}

/** Processo filho que grava tudo que chega no stdin em um arquivo. */
function sinkConfig(outFile: string) {
  return {
    command: process.execPath,
    args: [
      '-e',
      'const fs = require("fs"); process.stdin.pipe(fs.createWriteStream(process.argv[1]))',
      outFile,
    ],
    env: process.env,
  }
}

/** Processo filho que aborta imediatamente, como psql com ON_ERROR_STOP=1. */
function failFastConfig() {
  return {
    command: process.execPath,
    args: [
      '-e',
      'process.stderr.write("ERRO: syntax error at or near \\"SELEC\\""); process.exit(1)',
    ],
    env: process.env,
  }
}

async function runRestore(
  config: ReturnType<typeof sinkConfig>,
  stages: Array<Readable | Transform>
): Promise<RestoreResult> {
  const service = new RestoreService()
  return (await (service as any).executeRestore(config, stages, 'db_teste')) as RestoreResult
}

test.group('RestoreService - pipeline de restauracao', () => {
  test('entrega o SQL descomprimido e filtrado ao processo', async ({ assert }) => {
    const outFile = join(tmpdir(), `restore-sink-${Date.now()}.sql`)
    const sql = ['CREATE TABLE a (id int);', 'GRANT ALL ON a TO joao;', 'INSERT INTO a VALUES (1);']
    const gzipped = gzipSync(Buffer.from(sql.join('\n') + '\n', 'utf8'))

    const service = new RestoreService()
    const filters = (service as any).buildFilterStages('postgresql', {
      mode: 'full',
      noPrivileges: true,
    }) as Transform[]

    const result = await runRestore(sinkConfig(outFile), [
      Readable.from([gzipped]),
      createGunzip(),
      ...filters,
    ])

    try {
      assert.isTrue(result.success, `restore falhou: ${result.error}`)

      const received = readFileSync(outFile, 'utf8')
      assert.include(received, 'CREATE TABLE a (id int);')
      assert.include(received, 'INSERT INTO a VALUES (1);')
      // O filtro noPrivileges removeu o GRANT.
      assert.notInclude(received, 'GRANT ALL')
    } finally {
      await unlink(outFile).catch(() => {})
    }
  }).timeout(30_000)

  test('falha de leitura no meio da cadeia destroi o stream de origem', async ({ assert }) => {
    const corruptFile = join(tmpdir(), `restore-corrompido-${Date.now()}.gz`)
    const outFile = join(tmpdir(), `restore-sink-${Date.now()}.sql`)

    // Conteudo que nao e gzip valido: o gunzip do meio da cadeia vai falhar.
    writeFileSync(corruptFile, Buffer.from('isto definitivamente nao e um gzip valido'))

    const source = createReadStream(corruptFile)

    const result = await runRestore(sinkConfig(outFile), [source, createGunzip()])

    try {
      // Nao pode reportar sucesso: o processo pode ter saido com 0 sem receber
      // dado nenhum, mas o backup nao foi aplicado.
      assert.isFalse(result.success)
      assert.include(result.error ?? '', 'Erro ao ler arquivo de backup')

      // O ponto do pipeline(): a origem e fechada em cascata. Com .pipe()
      // encadeado, este descritor ficaria aberto ate o GC.
      assert.isTrue(source.destroyed, 'stream de origem deveria ter sido destruido')
    } finally {
      await unlink(corruptFile).catch(() => {})
      await unlink(outFile).catch(() => {})
    }
  }).timeout(30_000)

  test('processo que aborta antes de consumir tudo reporta o erro do banco', async ({ assert }) => {
    // 4 MB garantem que a escrita no stdin continue depois do processo morrer.
    const payload = Buffer.alloc(4 * 1024 * 1024, 'A')
    const result = await runRestore(failFastConfig() as any, [Readable.from([payload])])

    assert.isFalse(result.success)
    assert.equal(result.exitCode, 1)
    // A mensagem util e a do banco, nao o EPIPE do nosso lado do cano.
    assert.include(result.error ?? '', 'syntax error')
    assert.notInclude(result.error ?? '', 'EPIPE')
  }).timeout(30_000)

  test('binario inexistente reporta erro de execucao', async ({ assert }) => {
    const result = await runRestore(
      { command: 'psql-que-nao-existe', args: [], env: process.env } as any,
      [Readable.from([Buffer.from('SELECT 1;')])]
    )

    assert.isFalse(result.success)
    assert.include(result.error ?? '', 'psql-que-nao-existe')
    assert.include(result.error ?? '', 'PATH')
  }).timeout(30_000)
})
