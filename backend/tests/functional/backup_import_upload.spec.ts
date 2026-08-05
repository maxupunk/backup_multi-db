import { test } from '@japa/runner'
import { unlink } from 'node:fs/promises'
import { existsSync } from 'node:fs'

import Backup from '#models/backup'
import User from '#models/user'
import { getBackupStoragePath } from '#config/storage_paths'
import { StorageDestinationService } from '#services/storage_destination_service'

const SQL_CONTENT = [
  '-- Dump de teste',
  'CREATE TABLE clientes (id integer, nome text);',
  "INSERT INTO clientes VALUES (1, 'josé');",
  '',
].join('\n')

/**
 * Cobre o upload multipart de ponta a ponta.
 *
 * Esta rota é a ÚNICA listada em `autoProcess` no config do bodyparser — se
 * alguém alterar o padrão da rota ou a lista, `request.file('file')` passa a
 * devolver `null` e a importação quebra silenciosamente. Sem este teste, essa
 * regressão só apareceria em produção.
 */
test.group('Backup import - upload multipart', (group) => {
  let user: User
  const createdPaths: string[] = []

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Import User',
      email: `import_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })

    return async () => {
      for (const path of createdPaths.splice(0)) {
        if (existsSync(path)) {
          await unlink(path).catch(() => {})
        }
      }
      await Backup.query().delete()
    }
  })

  test('arquivo enviado e processado e registrado como backup', async ({ client, assert }) => {
    const token = await User.accessTokens.create(user)

    const response = await client
      .post('/api/backups/import')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .file('file', Buffer.from(SQL_CONTENT, 'utf8'), { filename: 'dump-de-teste.sql' })
      .field('databaseName', 'loja')
      .field('verifyIntegrity', 'false')

    response.assertStatus(201)
    response.assertBodyContains({ success: true })

    const body = response.body()
    assert.exists(body.data?.backup?.id, `resposta inesperada: ${JSON.stringify(body)}`)
    assert.equal(body.data.fileSize, Buffer.byteLength(SQL_CONTENT, 'utf8'))
    assert.isString(body.data.checksum)

    const backup = await Backup.find(body.data.backup.id)
    assert.exists(backup)
    assert.equal(backup!.databaseName, 'loja')
    assert.equal(backup!.status, 'completed')

    if (backup?.filePath) {
      createdPaths.push(StorageDestinationService.getLocalFullPath(null, backup.filePath))
    }
  }).timeout(30_000)

  test('requisicao sem arquivo e rejeitada com mensagem clara', async ({ client }) => {
    const token = await User.accessTokens.create(user)

    const response = await client
      .post('/api/backups/import')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .field('databaseName', 'loja')

    response.assertStatus(422)
    response.assertBodyContains({ success: false })
  }).timeout(30_000)

  test('extensao nao suportada e recusada', async ({ client }) => {
    const token = await User.accessTokens.create(user)

    const response = await client
      .post('/api/backups/import')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .file('file', Buffer.from('conteudo qualquer', 'utf8'), { filename: 'arquivo.exe' })
      .field('databaseName', 'loja')

    response.assertStatus(422)
  }).timeout(30_000)
})

test.group('Backup import - configuracao do bodyparser', () => {
  test('auto-processamento cobre exatamente a rota de import', async ({ assert }) => {
    const { default: bodyParserConfig } = await import('#config/bodyparser')
    const autoProcess = bodyParserConfig.multipart.autoProcess

    assert.isArray(autoProcess, 'autoProcess deveria listar rotas especificas, nao habilitar tudo')
    assert.deepEqual(autoProcess, ['/api/backups/import'])
  })

  test('caminho de storage do backup esta configurado', ({ assert }) => {
    assert.isString(getBackupStoragePath())
  })
})
