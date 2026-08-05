import { test } from '@japa/runner'

import StorageDestination from '#models/storage_destination'
import { EncryptionService } from '#services/encryption_service'

test.group('StorageDestination - memoizacao da config', (group) => {
  group.each.setup(async () => {
    return async () => {
      await StorageDestination.query().delete()
    }
  })

  test('descriptografa uma unica vez por instancia', async ({ assert }) => {
    const destination = await StorageDestination.create({
      name: `Memo ${Date.now()}`,
      type: 's3',
      status: 'active',
      isDefault: false,
      configEncrypted: JSON.stringify({
        type: 's3',
        bucket: 'meu-bucket',
        region: 'us-east-1',
        accessKeyId: 'chave',
        secretAccessKey: 'segredo',
        prefix: 'backups',
      }),
    })

    let decryptCalls = 0
    const originalDecrypt = EncryptionService.decrypt
    EncryptionService.decrypt = (value: string) => {
      decryptCalls++
      return originalDecrypt.call(EncryptionService, value)
    }

    try {
      const first = destination.getDecryptedConfig()
      const second = destination.getDecryptedConfig()
      const third = destination.getDecryptedConfig()

      assert.equal(decryptCalls, 1, 'config deveria ser descriptografada uma unica vez')
      assert.deepEqual(first, second)
      assert.deepEqual(second, third)
      assert.equal(first?.type, 's3')
    } finally {
      EncryptionService.decrypt = originalDecrypt
    }
  })

  test('alterar a config invalida o cache', async ({ assert }) => {
    const destination = await StorageDestination.create({
      name: `Memo invalidacao ${Date.now()}`,
      type: 'local',
      status: 'active',
      isDefault: false,
      configEncrypted: JSON.stringify({ type: 'local', basePath: '/dados/antigo' }),
    })

    const before = destination.getDecryptedConfig()
    assert.equal(before?.type === 'local' ? before.basePath : null, '/dados/antigo')

    destination.setConfig({ type: 'local', basePath: '/dados/novo' })
    await destination.save()

    const after = destination.getDecryptedConfig()
    assert.equal(after?.type === 'local' ? after.basePath : null, '/dados/novo')
  })

  test('instancias diferentes do mesmo registro nao compartilham cache', async ({ assert }) => {
    const created = await StorageDestination.create({
      name: `Memo instancias ${Date.now()}`,
      type: 'local',
      status: 'active',
      isDefault: false,
      configEncrypted: JSON.stringify({ type: 'local', basePath: '/dados/compartilhado' }),
    })

    const reloaded = await StorageDestination.findOrFail(created.id)
    const config = reloaded.getDecryptedConfig()

    assert.equal(config?.type === 'local' ? config.basePath : null, '/dados/compartilhado')
  })
})
