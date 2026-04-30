import app from '@adonisjs/core/services/app'
import { defineConfig } from '@adonisjs/lucid'
import { getSqliteDatabasePath } from '#config/storage_paths'
import { createSqliteAfterCreateHook } from '#services/sqlite_runtime_config'

const sqliteFilename = app.inTest ? ':memory:' : getSqliteDatabasePath()

const dbConfig = defineConfig({
  connection: 'sqlite',
  connections: {
    sqlite: {
      client: 'better-sqlite3',
      connection: {
        filename: sqliteFilename,
      },
      useNullAsDefault: true,
      pool: {
        afterCreate: createSqliteAfterCreateHook(sqliteFilename),
      },
      migrations: {
        naturalSort: true,
        paths: ['database/migrations'],
      },
    },
  },
})

export default dbConfig
