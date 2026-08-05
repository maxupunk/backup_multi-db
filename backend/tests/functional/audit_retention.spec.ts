import { test } from '@japa/runner'
import { DateTime } from 'luxon'
import db from '@adonisjs/lucid/services/db'

import AuditLog from '#models/audit_log'
import {
  AUDIT_LOGS_TABLE,
  AuditRetentionService,
  DEFAULT_AUDIT_RETENTION_DAYS,
} from '#services/audit_retention_service'

async function createAuditLog(createdAt: DateTime, description: string): Promise<number> {
  const log = await AuditLog.create({
    action: 'settings.updated',
    entityType: 'settings',
    entityId: null,
    entityName: null,
    description,
    status: 'success',
  })

  // `autoCreate` define createdAt no insert; reposicionar no tempo desejado.
  await db
    .from(AUDIT_LOGS_TABLE)
    .where('id', log.id)
    .update({ created_at: createdAt.toFormat('yyyy-LL-dd HH:mm:ss') })

  return log.id
}

test.group('Audit retention', (group) => {
  group.each.setup(async () => {
    await AuditLog.query().delete()
    return async () => {
      await AuditLog.query().delete()
    }
  })

  test('remove apenas logs mais antigos que a janela de retencao', async ({ assert }) => {
    const now = DateTime.now()
    const janela = DEFAULT_AUDIT_RETENTION_DAYS

    // Datas derivadas da janela: se o padrao mudar, o teste continua exercitando
    // a borda em vez de virar um numero solto que passa por acaso.
    const antigo = await createAuditLog(now.minus({ days: janela * 4 }), 'muito antigo')
    const limiar = await createAuditLog(now.minus({ days: janela + 1 }), 'logo antes do corte')
    const dentro = await createAuditLog(now.minus({ days: janela - 1 }), 'logo depois do corte')
    const hoje = await createAuditLog(now, 'de hoje')

    const result = await AuditRetentionService.prune(now)

    assert.equal(result.retentionDays, DEFAULT_AUDIT_RETENTION_DAYS)
    assert.equal(result.deleted, 2)
    assert.isFalse(result.truncated)

    const remaining = await AuditLog.query().select('id')
    const remainingIds = remaining.map((log) => log.id)

    assert.notInclude(remainingIds, antigo)
    assert.notInclude(remainingIds, limiar)
    assert.include(remainingIds, dentro)
    assert.include(remainingIds, hoje)
  })

  test('a janela padrao fica na faixa de diagnostico (15 a 30 dias)', ({ assert }) => {
    assert.isAtLeast(DEFAULT_AUDIT_RETENTION_DAYS, 15)
    assert.isAtMost(DEFAULT_AUDIT_RETENTION_DAYS, 30)
  })

  test('nao remove nada quando tudo esta dentro da janela', async ({ assert }) => {
    const now = DateTime.now()
    await createAuditLog(now.minus({ days: 5 }), 'recente')

    const result = await AuditRetentionService.prune(now)

    assert.equal(result.deleted, 0)
    assert.lengthOf(await AuditLog.query().select('id'), 1)
  })

  test('retencao zero desliga a poda', async ({ assert }) => {
    const now = DateTime.now()
    await createAuditLog(now.minus({ days: 3650 }), 'antiquissimo')

    const originalResolve = AuditRetentionService.resolveRetentionDays
    AuditRetentionService.resolveRetentionDays = () => 0

    try {
      const result = await AuditRetentionService.prune(now)

      assert.equal(result.retentionDays, 0)
      assert.equal(result.deleted, 0)
      assert.lengthOf(await AuditLog.query().select('id'), 1)
    } finally {
      AuditRetentionService.resolveRetentionDays = originalResolve
    }
  })

  test('valor invalido de configuracao cai no padrao conservador', ({ assert }) => {
    assert.equal(
      AuditRetentionService.resolveRetentionDays(undefined),
      DEFAULT_AUDIT_RETENTION_DAYS
    )
    assert.equal(
      AuditRetentionService.resolveRetentionDays(-5 as any),
      DEFAULT_AUDIT_RETENTION_DAYS
    )
    assert.equal(
      AuditRetentionService.resolveRetentionDays('abc' as any),
      DEFAULT_AUDIT_RETENTION_DAYS
    )
    assert.equal(AuditRetentionService.resolveRetentionDays(30 as any), 30)
    assert.equal(AuditRetentionService.resolveRetentionDays(0 as any), 0)
  })
})
