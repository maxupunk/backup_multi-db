type SqliteConnectionLike = {
  pragma?: (statement: string) => unknown
  exec?: (statement: string) => unknown
}

const DISK_SQLITE_PRAGMAS = ['journal_mode = WAL', 'synchronous = NORMAL'] as const

export function applySqliteRuntimePragmas(
  connection: SqliteConnectionLike,
  filename: string
): void {
  if (filename === ':memory:') {
    return
  }

  if (typeof connection.pragma === 'function') {
    for (const pragma of DISK_SQLITE_PRAGMAS) {
      connection.pragma(pragma)
    }

    return
  }

  if (typeof connection.exec === 'function') {
    connection.exec(DISK_SQLITE_PRAGMAS.map((pragma) => `PRAGMA ${pragma};`).join(' '))
  }
}

export function createSqliteAfterCreateHook(filename: string) {
  return (
    connection: SqliteConnectionLike,
    done: (error: Error | null, connection: unknown) => void
  ) => {
    try {
      applySqliteRuntimePragmas(connection, filename)
      done(null, connection)
    } catch (error) {
      done(error as Error, connection)
    }
  }
}
