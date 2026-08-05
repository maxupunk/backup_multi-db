import { test } from '@japa/runner'
import { DateTime } from 'luxon'

import Backup from '#models/backup'
import Connection from '#models/connection'
import ConnectionDatabase from '#models/connection_database'
import { BackupRetentionPlanner } from '#services/backup_retention_planner'
import { RetentionService } from '#services/retention_service'

const CONFIG = { daily: 7, weekly: 4, monthly: 12, yearly: 5 }

/**
 * Cria um cenário com backups espalhados no tempo, cobrindo todas as faixas do
 * GFS (mesmo dia, dias, semanas, meses, anos) e mais de um backup por bucket,
 * que é onde a deduplicação acontece.
 */
async function seedBackups(): Promise<void> {
  const connection = await Connection.create({
    name: `Retencao ${Date.now()}`,
    type: 'postgresql',
    host: 'localhost',
    port: 5432,
    username: 'user',
    passwordEncrypted: 'password',
    status: 'active',
  })

  const databases = await Promise.all([
    ConnectionDatabase.create({
      connectionId: connection.id,
      databaseName: 'loja',
      enabled: true,
    }),
    ConnectionDatabase.create({
      connectionId: connection.id,
      databaseName: 'financeiro',
      enabled: true,
    }),
  ])

  const now = DateTime.now()
  const offsets = [
    { hours: 1 },
    { hours: 2 },
    { hours: 3 },
    { days: 1 },
    { days: 1, hours: 6 },
    { days: 2 },
    { days: 5 },
    { days: 9 },
    { days: 16 },
    { days: 40 },
    { days: 70 },
    { days: 200 },
    { days: 400 },
    { days: 800 },
  ]

  for (const [index, offset] of offsets.entries()) {
    const createdAt = now.minus(offset)

    const database = databases[index % databases.length]

    const backup = new Backup()
    backup.connectionId = connection.id
    // Alterna entre backups com e sem vínculo de database para exercitar os dois
    // ramos de escopo do planner.
    backup.connectionDatabaseId = index % 3 === 0 ? null : database.id
    backup.databaseName = database.databaseName
    backup.storageDestinationId = null
    backup.status = index === 4 ? 'failed' : 'completed'
    backup.trigger = 'scheduled'
    backup.compressed = true
    backup.protected = false
    backup.retentionType = 'hourly'
    backup.filePath = `retencao/backup-${index}.sql.gz`
    backup.fileName = `backup-${index}.sql.gz`
    backup.fileSize = 1024
    backup.createdAt = createdAt
    backup.updatedAt = createdAt
    await backup.save()

    // `autoCreate` sobrescreve createdAt no insert; forçar o valor desejado.
    await Backup.query()
      .where('id', backup.id)
      .update({ created_at: createdAt.toUTC().toFormat('yyyy-LL-dd HH:mm:ss') })
  }
}

test.group('Retention - projecao enxuta vs modelos Lucid', (group) => {
  group.each.setup(async () => {
    await Backup.query().delete()
    await seedBackups()
    return async () => {
      await Backup.query().delete()
    }
  })

  test('a projecao produz exatamente o mesmo plano dos modelos completos', async ({ assert }) => {
    const service = new RetentionService(CONFIG)
    const planner = new BackupRetentionPlanner(CONFIG)
    const now = DateTime.now()

    // Caminho antigo: modelos Lucid completos.
    const models = await Backup.query()
      .where('protected', false)
      .whereNotIn('status', ['pending', 'running'])
      .orderBy('createdAt', 'desc')

    // Caminho novo: projeção com as colunas usadas.
    const candidates = await (service as any).loadPrunableBackups()

    assert.equal(candidates.length, models.length, 'quantidade de candidatos divergente')
    assert.isAbove(models.length, 0, 'cenário sem backups não testa nada')

    const planFromModels = planner.plan(models, now)
    const planFromProjection = planner.plan(candidates, now)

    assert.deepEqual(
      [...planFromProjection.toDelete].sort((a, b) => a - b),
      [...planFromModels.toDelete].sort((a, b) => a - b),
      'conjunto de exclusao divergente entre os dois caminhos'
    )

    assert.deepEqual(
      [...planFromProjection.retained.entries()].sort(([a], [b]) => a - b),
      [...planFromModels.retained.entries()].sort(([a], [b]) => a - b),
      'plano de retencao divergente entre os dois caminhos'
    )
  })

  test('createdAt da projecao bate com o do modelo ao segundo', async ({ assert }) => {
    const service = new RetentionService(CONFIG)

    const models = await Backup.query().orderBy('id', 'asc')
    const candidates = (await (service as any).loadPrunableBackups()) as Array<{
      id: number
      createdAt: DateTime
    }>

    const byId = new Map(candidates.map((candidate) => [candidate.id, candidate]))

    for (const model of models) {
      const candidate = byId.get(model.id)
      assert.exists(candidate, `backup ${model.id} ausente na projecao`)
      assert.isTrue(candidate!.createdAt.isValid, `createdAt invalido no backup ${model.id}`)

      // Tolerância de 1s cobre truncamento de milissegundos no armazenamento.
      const deltaMs = Math.abs(candidate!.createdAt.toMillis() - model.createdAt.toMillis())
      assert.isBelow(deltaMs, 1000, `createdAt divergente no backup ${model.id}`)
    }
  })

  test('promocao de retencao persiste os mesmos tipos do plano', async ({ assert }) => {
    const service = new RetentionService(CONFIG)
    const planner = new BackupRetentionPlanner(CONFIG)

    const candidates = await (service as any).loadPrunableBackups()
    const plan = planner.plan(candidates, DateTime.now())

    const promoted = await (service as any).syncRetentionTypes(candidates, plan.retained)

    const persisted = await Backup.query().whereIn('id', [...plan.retained.keys()])

    for (const backup of persisted) {
      assert.equal(
        backup.retentionType,
        plan.retained.get(backup.id),
        `retentionType nao persistido no backup ${backup.id}`
      )
    }

    // Rodar de novo não deve promover nada: o estado já convergiu.
    const secondPass = await (service as any).syncRetentionTypes(
      await (service as any).loadPrunableBackups(),
      plan.retained
    )

    assert.equal(secondPass, 0, 'sincronizacao nao e idempotente')
    assert.isAtLeast(promoted, 0)
  })
})
