import { test } from '@japa/runner'
import User from '#models/user'
import SystemSetting from '#models/system_setting'
import { BackupRetentionPolicyService } from '#services/backup_retention_policy_service'
import { SchedulerService } from '#services/scheduler_service'

test.group('System backup retention', (group) => {
  let user: User

  group.each.setup(async () => {
    user = await User.create({
      fullName: 'Retention User',
      email: `retention_${Date.now()}@example.com`,
      password: 'Password123!',
      isActive: true,
    })
  })

  test('get backup retention policy returns persisted or default values', async ({
    client,
    assert,
  }) => {
    const token = await User.accessTokens.create(user)
    const policyService = new BackupRetentionPolicyService()
    const expectedPolicy = policyService.getDefaultPolicy()

    const response = await client
      .get('/api/system/backup-retention')
      .header('Authorization', `Bearer ${token.value!.release()}`)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {
        ...expectedPolicy,
        defaultPruneCron: expectedPolicy.pruneCron,
      },
    })

    const setting = await SystemSetting.findBy('name', 'backup_retention_policy')
    assert.exists(setting)
    assert.deepEqual(setting?.value, expectedPolicy)
  })

  test('update backup retention policy persists new values', async ({ client, assert }) => {
    const token = await User.accessTokens.create(user)
    const payload = {
      daily: 14,
      weekly: 6,
      monthly: 18,
      yearly: 8,
      pruneCron: '0 */6 * * *',
    }

    const response = await client
      .put('/api/system/backup-retention')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json(payload)

    response.assertStatus(200)
    response.assertBodyContains({
      success: true,
      data: {
        ...payload,
        defaultPruneCron: new BackupRetentionPolicyService().getDefaultPolicy().pruneCron,
      },
    })

    const setting = await SystemSetting.findBy('name', 'backup_retention_policy')
    assert.exists(setting)
    assert.deepEqual(setting?.value, payload)
  })

  test('update backup retention policy rejects invalid cron expressions', async ({ client }) => {
    const token = await User.accessTokens.create(user)

    const response = await client
      .put('/api/system/backup-retention')
      .header('Authorization', `Bearer ${token.value!.release()}`)
      .json({
        daily: 7,
        weekly: 4,
        monthly: 12,
        yearly: 5,
        pruneCron: 'invalid cron',
      })

    response.assertStatus(422)
    response.assertBodyContains({
      success: false,
      message: 'Expressão cron inválida para o prune automático',
    })
  })

  test('run backup retention returns execution summary', async ({ client, assert }) => {
    const token = await User.accessTokens.create(user)
    const originalRunRetentionNow = SchedulerService.prototype.runRetentionNow

    SchedulerService.prototype.runRetentionNow = async function stubRunRetentionNow() {
      return {
        deleted: 1,
        promoted: 2,
        protected: 3,
        errors: [],
        deletedBackups: [
          {
            id: 99,
            connectionId: 10,
            connectionDatabaseId: 20,
            databaseName: 'archive_db',
            fileName: 'archive_db_2026-04-28.sql.gz',
            retentionType: 'daily',
            createdAt: '2026-04-28T23:00:00.000Z',
          },
        ],
      }
    }

    try {
      const response = await client
        .post('/api/system/backup-retention/run')
        .header('Authorization', `Bearer ${token.value!.release()}`)

      response.assertStatus(200)
      response.assertBodyContains({
        success: true,
        data: {
          deleted: 1,
          promoted: 2,
          protected: 3,
        },
      })

      const body = response.body()
      assert.deepEqual(body.data.deletedBackups, [
        {
          id: 99,
          connectionId: 10,
          connectionDatabaseId: 20,
          databaseName: 'archive_db',
          fileName: 'archive_db_2026-04-28.sql.gz',
          retentionType: 'daily',
          createdAt: '2026-04-28T23:00:00.000Z',
        },
      ])
    } finally {
      SchedulerService.prototype.runRetentionNow = originalRunRetentionNow
    }
  })
})
