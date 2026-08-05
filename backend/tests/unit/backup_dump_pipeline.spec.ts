import { test } from '@japa/runner'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { unlink } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { gunzipSync } from 'node:zlib'

import { BackupService } from '#services/backup_service'
import { PROCESS_OUTPUT_TRUNCATION_SUFFIX } from '#services/process_output_buffer'

type DumpResult = {
  success: boolean
  filePath?: string
  fileName?: string
  fileSize?: number
  checksum?: string
  error?: string
  exitCode?: number
}

/**
 * Roda o pipeline real de dump usando o proprio Node como "banco de dados":
 * o processo filho escreve um payload conhecido em stdout, exatamente como um
 * pg_dump/mysqldump faria.
 */
async function runDump(
  scriptArgs: string[],
  fileName: string
): Promise<{ result: DumpResult; fullPath: string }> {
  const service = new BackupService()
  const fullPath = join(tmpdir(), fileName)

  const result = (await (service as any).executeDumpProcess(
    { command: process.execPath, args: scriptArgs, env: process.env },
    fullPath,
    fileName,
    fileName
  )) as DumpResult

  return { result, fullPath }
}

test.group('BackupService - pipeline de dump', () => {
  test('preserva o conteudo e o checksum do dump nao comprimido', async ({ assert }) => {
    // 4 MB atravessam varios chunks e buffers internos do gzip — o suficiente
    // para exercitar o caminho de backpressure de ponta a ponta.
    const chunkSize = 1024 * 1024
    const chunkCount = 4
    const fileName = `dump-integridade-${Date.now()}.sql.gz`

    const { result, fullPath } = await runDump(
      [
        '-e',
        `const b = Buffer.alloc(${chunkSize}, 'a'); for (let i = 0; i < ${chunkCount}; i++) process.stdout.write(b)`,
      ],
      fileName
    )

    try {
      assert.isTrue(result.success, `dump falhou: ${result.error}`)

      const expectedPayload = Buffer.alloc(chunkSize * chunkCount, 'a')
      const expectedChecksum = createHash('sha256').update(expectedPayload).digest('hex')

      // O arquivo gravado descomprime exatamente para o que o processo emitiu.
      const decompressed = gunzipSync(readFileSync(fullPath))
      assert.equal(decompressed.length, expectedPayload.length)
      assert.isTrue(decompressed.equals(expectedPayload), 'conteudo descomprimido divergente')

      // O checksum e' calculado sobre os bytes NAO comprimidos.
      assert.equal(result.checksum, expectedChecksum)
      assert.equal(result.fileSize, readFileSync(fullPath).length)
    } finally {
      await unlink(fullPath).catch(() => {})
    }
  }).timeout(30_000)

  test('dump vazio produz arquivo valido e checksum do conteudo vazio', async ({ assert }) => {
    const fileName = `dump-vazio-${Date.now()}.sql.gz`
    const { result, fullPath } = await runDump(['-e', 'process.exit(0)'], fileName)

    try {
      assert.isTrue(result.success, `dump falhou: ${result.error}`)
      assert.equal(gunzipSync(readFileSync(fullPath)).length, 0)
      assert.equal(result.checksum, createHash('sha256').update(Buffer.alloc(0)).digest('hex'))
    } finally {
      await unlink(fullPath).catch(() => {})
    }
  }).timeout(30_000)

  test('exit code diferente de zero vira falha com o stderr do processo', async ({ assert }) => {
    const fileName = `dump-falha-${Date.now()}.sql.gz`
    const { result, fullPath } = await runDump(
      ['-e', 'process.stderr.write("pg_dump: erro de autenticacao"); process.exit(1)'],
      fileName
    )

    try {
      assert.isFalse(result.success)
      assert.equal(result.exitCode, 1)
      assert.include(result.error ?? '', 'pg_dump: erro de autenticacao')
    } finally {
      await unlink(fullPath).catch(() => {})
    }
  }).timeout(30_000)

  test('stderr gigante e truncado em vez de crescer sem limite', async ({ assert }) => {
    const fileName = `dump-stderr-${Date.now()}.sql.gz`
    // 2 MB de stderr contra um teto de captura de 256 KB.
    const { result, fullPath } = await runDump(
      [
        '-e',
        'const b = Buffer.alloc(1024 * 1024, "x"); process.stderr.write(b); process.stderr.write(b); process.exit(1)',
      ],
      fileName
    )

    try {
      assert.isFalse(result.success)
      assert.isBelow((result.error ?? '').length, 1024 * 1024)
      assert.include(result.error ?? '', PROCESS_OUTPUT_TRUNCATION_SUFFIX.trim())
    } finally {
      await unlink(fullPath).catch(() => {})
    }
  }).timeout(30_000)

  test('binario inexistente reporta erro de execucao sem travar a promise', async ({ assert }) => {
    const service = new BackupService()
    const fileName = `dump-sem-binario-${Date.now()}.sql.gz`
    const fullPath = join(tmpdir(), fileName)

    const result = (await (service as any).executeDumpProcess(
      {
        command: 'binario-que-nao-existe-em-lugar-nenhum',
        args: [],
        env: process.env,
      },
      fullPath,
      fileName,
      fileName
    )) as DumpResult

    try {
      assert.isFalse(result.success)
      assert.include(result.error ?? '', 'binario-que-nao-existe-em-lugar-nenhum')
      assert.include(result.error ?? '', 'PATH')
    } finally {
      await unlink(fullPath).catch(() => {})
    }
  }).timeout(30_000)
})
