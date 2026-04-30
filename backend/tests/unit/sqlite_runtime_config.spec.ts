import { test } from '@japa/runner'

import {
  applySqliteRuntimePragmas,
  createSqliteAfterCreateHook,
} from '#services/sqlite_runtime_config'

test.group('SQLite runtime config', () => {
  test('applies WAL pragmas through pragma() for disk-backed databases', ({ assert }) => {
    const statements: string[] = []

    applySqliteRuntimePragmas(
      {
        pragma: (statement: string) => {
          statements.push(statement)
        },
      },
      'storage/database/app.sqlite3'
    )

    assert.deepEqual(statements, ['journal_mode = WAL', 'synchronous = NORMAL'])
  })

  test('skips WAL pragmas for in-memory databases', ({ assert }) => {
    const statements: string[] = []

    applySqliteRuntimePragmas(
      {
        pragma: (statement: string) => {
          statements.push(statement)
        },
      },
      ':memory:'
    )

    assert.deepEqual(statements, [])
  })

  test('falls back to exec() when pragma() is unavailable', ({ assert }) => {
    const executed: string[] = []

    applySqliteRuntimePragmas(
      {
        exec: (statement: string) => {
          executed.push(statement)
        },
      },
      'storage/database/app.sqlite3'
    )

    assert.deepEqual(executed, ['PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;'])
  })

  test('afterCreate hook forwards connection after applying pragmas', ({ assert }) => {
    const statements: string[] = []
    const connection = {
      pragma: (statement: string) => {
        statements.push(statement)
      },
    }

    let callbackError: Error | null = null
    let callbackConnection: unknown = null

    createSqliteAfterCreateHook('storage/database/app.sqlite3')(connection, (error, created) => {
      callbackError = error
      callbackConnection = created
    })

    assert.isNull(callbackError)
    assert.strictEqual(callbackConnection, connection)
    assert.deepEqual(statements, ['journal_mode = WAL', 'synchronous = NORMAL'])
  })
})
