import { test } from '@japa/runner'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import User from '#models/user'
import AuditLog from '#models/audit_log'
import { DiagnosticsFileService } from '#services/diagnostics_file_service'

async function createUser(isAdmin: boolean): Promise<User> {
  return User.create({
    fullName: isAdmin ? 'Admin Diag' : 'Comum Diag',
    email: `diag_${isAdmin ? 'admin' : 'user'}_${Date.now()}_${Math.random()}@example.com`,
    password: 'Password123!',
    isActive: true,
    isAdmin,
  })
}

async function tokenFor(user: User): Promise<string> {
  const token = await User.accessTokens.create(user)
  return token.value!.release()
}

test.group('Diagnosticos - protecao de caminho', (group) => {
  let directory: string
  let originalGetDirectory: () => string

  group.each.setup(async () => {
    directory = await mkdtemp(join(tmpdir(), 'diag-'))
    originalGetDirectory = DiagnosticsFileService.getDirectory
    ;(DiagnosticsFileService as any).getDirectory = () => directory

    await writeFile(join(directory, 'Heap.20260805.1.heapsnapshot'), 'conteudo-heap')
    await writeFile(join(directory, 'perfil.cpuprofile'), 'conteudo-cpu')
    await writeFile(join(directory, 'leia-me.txt'), 'nao deve aparecer')

    return async () => {
      ;(DiagnosticsFileService as any).getDirectory = originalGetDirectory
      await rm(directory, { recursive: true, force: true })
    }
  })

  test('lista apenas artefatos de diagnostico conhecidos', async ({ assert }) => {
    const files = await DiagnosticsFileService.list()
    const names = files.map((file) => file.name)

    assert.includeMembers(names, ['Heap.20260805.1.heapsnapshot', 'perfil.cpuprofile'])
    assert.notInclude(names, 'leia-me.txt')
    assert.isAbove(files[0].sizeBytes, 0)
  })

  test('resolve o caminho de um artefato existente', ({ assert }) => {
    const resolved = DiagnosticsFileService.resolvePath('Heap.20260805.1.heapsnapshot')

    assert.isString(resolved)
    assert.include(resolved!, directory)
  })

  test('recusa qualquer tentativa de escapar do diretorio', ({ assert }) => {
    const tentativas = [
      '../../../etc/passwd',
      '..\\..\\Windows\\System32\\config\\SAM',
      '../secrets.heapsnapshot',
      'subdir/arquivo.heapsnapshot',
      'subdir\\arquivo.heapsnapshot',
      '/etc/shadow',
      'C:\\Windows\\win.ini',
      '',
      '   ',
    ]

    for (const tentativa of tentativas) {
      assert.isNull(
        DiagnosticsFileService.resolvePath(tentativa),
        `deveria recusar: ${JSON.stringify(tentativa)}`
      )
    }
  })

  test('recusa extensao fora da allowlist mesmo existindo no diretorio', ({ assert }) => {
    assert.isNull(DiagnosticsFileService.resolvePath('leia-me.txt'))
  })

  test('recusa artefato inexistente com extensao valida', ({ assert }) => {
    assert.isNull(DiagnosticsFileService.resolvePath('nao-existe.heapsnapshot'))
  })

  test('diretorio inexistente devolve lista vazia em vez de erro', async ({ assert }) => {
    ;(DiagnosticsFileService as any).getDirectory = () => join(directory, 'nao-existe')

    assert.deepEqual(await DiagnosticsFileService.list(), [])
    assert.isFalse(DiagnosticsFileService.directoryExists())
  })
})

test.group('Diagnosticos - endpoints', (group) => {
  let directory: string
  let originalGetDirectory: () => string

  group.each.setup(async () => {
    directory = await mkdtemp(join(tmpdir(), 'diag-api-'))
    originalGetDirectory = DiagnosticsFileService.getDirectory
    ;(DiagnosticsFileService as any).getDirectory = () => directory

    await writeFile(join(directory, 'Heap.teste.heapsnapshot'), 'segredo-do-heap')

    return async () => {
      ;(DiagnosticsFileService as any).getDirectory = originalGetDirectory
      await rm(directory, { recursive: true, force: true })
      await AuditLog.query().delete()
    }
  })

  test('usuario nao administrador nao lista artefatos', async ({ client }) => {
    const token = await tokenFor(await createUser(false))

    const response = await client
      .get('/api/system/diagnostics')
      .header('Authorization', `Bearer ${token}`)

    response.assertStatus(403)
  })

  test('usuario nao administrador nao baixa artefatos', async ({ client }) => {
    const token = await tokenFor(await createUser(false))

    const response = await client
      .get('/api/system/diagnostics/Heap.teste.heapsnapshot/download')
      .header('Authorization', `Bearer ${token}`)

    response.assertStatus(403)
  })

  test('administrador lista os artefatos e o diretorio', async ({ client, assert }) => {
    const token = await tokenFor(await createUser(true))

    const response = await client
      .get('/api/system/diagnostics')
      .header('Authorization', `Bearer ${token}`)

    response.assertStatus(200)

    const body = response.body()
    assert.equal(body.data.directory, directory)
    assert.isTrue(body.data.directoryExists)
    assert.lengthOf(body.data.files, 1)
    assert.equal(body.data.files[0].name, 'Heap.teste.heapsnapshot')
  })

  test('administrador baixa o artefato e o download fica auditado', async ({ client, assert }) => {
    const token = await tokenFor(await createUser(true))

    const response = await client
      .get('/api/system/diagnostics/Heap.teste.heapsnapshot/download')
      .header('Authorization', `Bearer ${token}`)

    response.assertStatus(200)
    assert.equal(
      response.header('content-disposition'),
      'attachment; filename="Heap.teste.heapsnapshot"'
    )

    const registro = await AuditLog.query()
      .where('action', 'diagnostics.downloaded')
      .orderBy('id', 'desc')
      .first()

    assert.exists(registro, 'download deveria gerar registro de auditoria')
    assert.equal(registro!.entityName, 'Heap.teste.heapsnapshot')
  })

  test('download com path traversal responde 404', async ({ client }) => {
    const token = await tokenFor(await createUser(true))

    const response = await client
      .get('/api/system/diagnostics/..%2F..%2Fpackage.json/download')
      .header('Authorization', `Bearer ${token}`)

    response.assertStatus(404)
  })

  test('administrador remove o artefato e a remocao fica auditada', async ({ client, assert }) => {
    const token = await tokenFor(await createUser(true))

    const response = await client
      .delete('/api/system/diagnostics/Heap.teste.heapsnapshot')
      .header('Authorization', `Bearer ${token}`)

    response.assertStatus(200)
    assert.deepEqual(await DiagnosticsFileService.list(), [])

    const registro = await AuditLog.query()
      .where('action', 'diagnostics.deleted')
      .orderBy('id', 'desc')
      .first()

    assert.exists(registro, 'remocao deveria gerar registro de auditoria')
  })
})
