import { test } from '@japa/runner'
import { DateTime } from 'luxon'

import { BackupRetentionPlanner } from '#services/backup_retention_planner'

test.group('Backup retention planner', () => {
  test('keeps one completed backup per hourly, daily, weekly, monthly and yearly bucket', ({
    assert,
  }) => {
    const planner = new BackupRetentionPlanner({
      daily: 2,
      weekly: 2,
      monthly: 12,
      yearly: 5,
    })

    const now = createValidDateTime('2026-04-30T02:00:00.000-03:00')
    const plan = planner.plan(
      [
        createBackup(1, '2026-04-30T01:40:00.000-03:00'),
        createBackup(2, '2026-04-30T01:10:00.000-03:00'),
        createBackup(3, '2026-04-29T23:50:00.000-03:00'),
        createBackup(4, '2026-04-29T08:00:00.000-03:00'),
        createBackup(5, '2026-04-28T22:00:00.000-03:00'),
        createBackup(6, '2026-04-21T21:00:00.000-03:00'),
        createBackup(7, '2026-04-20T20:00:00.000-03:00'),
        createBackup(8, '2026-03-15T12:00:00.000-03:00'),
        createBackup(9, '2026-03-02T12:00:00.000-03:00'),
        createBackup(10, '2025-06-10T12:00:00.000-03:00'),
        createBackup(11, '2024-05-10T12:00:00.000-03:00'),
        createBackup(12, '2020-05-10T12:00:00.000-03:00'),
      ],
      now
    )

    assert.deepEqual(
      Array.from(plan.retained.entries()).sort((left, right) => left[0] - right[0]),
      [
        [1, 'hourly'],
        [3, 'daily'],
        [5, 'daily'],
        [6, 'weekly'],
        [8, 'monthly'],
        [10, 'monthly'],
        [11, 'yearly'],
      ]
    )

    assert.deepEqual(
      plan.toDelete.sort((left, right) => left - right),
      [2, 4, 7, 9, 12]
    )
  })

  test('groups retention independently per database stream', ({ assert }) => {
    const planner = new BackupRetentionPlanner({
      daily: 1,
      weekly: 0,
      monthly: 0,
      yearly: 0,
    })

    const now = createValidDateTime('2026-04-30T02:00:00.000-03:00')
    const plan = planner.plan(
      [
        createBackup(1, '2026-04-29T23:30:00.000-03:00', { connectionDatabaseId: 10 }),
        createBackup(2, '2026-04-29T20:00:00.000-03:00', { connectionDatabaseId: 10 }),
        createBackup(3, '2026-04-29T22:00:00.000-03:00', { connectionDatabaseId: 20 }),
      ],
      now
    )

    assert.deepEqual(
      Array.from(plan.retained.entries()).sort((left, right) => left[0] - right[0]),
      [
        [1, 'daily'],
        [3, 'daily'],
      ]
    )

    assert.deepEqual(plan.toDelete, [2])
  })

  test('does not let failed backups replace successful restore points and only keeps them for the current day', ({
    assert,
  }) => {
    const planner = new BackupRetentionPlanner({
      daily: 1,
      weekly: 0,
      monthly: 0,
      yearly: 0,
    })

    const now = createValidDateTime('2026-04-30T10:00:00.000-03:00')
    const plan = planner.plan(
      [
        createBackup(1, '2026-04-30T09:10:00.000-03:00', { status: 'failed' }),
        createBackup(2, '2026-04-30T09:00:00.000-03:00'),
        createBackup(3, '2026-04-29T23:50:00.000-03:00', { status: 'failed' }),
        createBackup(4, '2026-04-29T23:00:00.000-03:00'),
      ],
      now
    )

    assert.deepEqual(
      Array.from(plan.retained.entries()).sort((left, right) => left[0] - right[0]),
      [
        [1, 'hourly'],
        [2, 'hourly'],
        [4, 'daily'],
      ]
    )

    assert.deepEqual(plan.toDelete, [3])
  })
})

function createBackup(
  id: number,
  createdAtIso: string,
  overrides?: Partial<{
    connectionId: number | null
    connectionDatabaseId: number | null
    databaseName: string
    status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'
    retentionType: 'hourly' | 'daily' | 'weekly' | 'monthly' | 'yearly'
  }>
) {
  return {
    id,
    connectionId: overrides?.connectionId ?? 1,
    connectionDatabaseId: overrides?.connectionDatabaseId ?? 1,
    databaseName: overrides?.databaseName ?? 'main_db',
    createdAt: createValidDateTime(createdAtIso),
    status: overrides?.status ?? 'completed',
    retentionType: overrides?.retentionType ?? 'hourly',
    metadata: null,
  }
}

function createValidDateTime(iso: string): DateTime<true> {
  const value = DateTime.fromISO(iso)

  if (!value.isValid) {
    throw new Error(`Invalid DateTime test fixture: ${iso}`)
  }

  return value
}
