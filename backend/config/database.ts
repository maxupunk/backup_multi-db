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
      /**
       * Uma unica conexao. O driver `better-sqlite3` e' sincrono — cada query
       * bloqueia o event loop ate terminar, entao conexoes adicionais nao
       * trazem paralelismo nenhum. O pool default do knex (min 2 / max 10)
       * apenas multiplicava o page cache do SQLite (`cache_size = -4096`,
       * 4 MB POR conexao) em ate 40 MB de RSS sem contrapartida.
       */
      pool: {
        min: 1,
        max: 1,
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
