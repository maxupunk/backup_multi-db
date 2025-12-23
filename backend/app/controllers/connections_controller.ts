import type { HttpContext } from '@adonisjs/core/http'
import { DateTime } from 'luxon'
import Connection from '#models/connection'
import ConnectionDatabase from '#models/connection_database'
import {
  createConnectionValidator,
  updateConnectionValidator,
  listConnectionsValidator,
  discoverDatabasesValidator,
} from '#validators/connection_validator'
import { BackupService } from '#services/backup_service'
import { AuditService } from '#services/audit_service'
import { NotificationService } from '#services/notification_service'

/**
 * Controller para gerenciamento de conexões de banco de dados
 */
export default class ConnectionsController {
  /**
   * GET /api/connections
   * Lista todas as conexões com paginação e filtros
   */
  async index({ request, response }: HttpContext) {
    const query = await request.validateUsing(listConnectionsValidator)

    const page = query.page || 1
    const limit = query.limit || 20

    const connectionsQuery = Connection.query().orderBy('name', 'asc')

    // Filtro por tipo de banco
    if (query.type) {
      connectionsQuery.where('type', query.type)
    }

    // Filtro por status
    if (query.status) {
      connectionsQuery.where('status', query.status)
    }

    // Busca por nome ou host
    if (query.search) {
      const searchTerm = `%${query.search}%`
      connectionsQuery.where((builder) => {
        builder.whereILike('name', searchTerm).orWhereILike('host', searchTerm)
      })
    }

    // Carregar databases e último backup de cada conexão
    connectionsQuery.preload('databases', (dbQuery) => {
      dbQuery.where('enabled', true).orderBy('databaseName', 'asc')
    })
    connectionsQuery.preload('backups', (backupsQuery) => {
      backupsQuery.orderBy('createdAt', 'desc').limit(1)
    })

    const connections = await connectionsQuery.paginate(page, limit)

    return response.ok({
      success: true,
      data: connections.serialize({
        relations: {
          databases: {
            fields: ['id', 'databaseName', 'enabled'],
          },
          backups: {
            fields: ['id', 'status', 'fileSize', 'databaseName', 'createdAt', 'finishedAt'],
          },
        },
      }),
    })
  }

  /**
   * POST /api/connections
   * Cria uma nova conexão com múltiplos databases
   */
  async store(ctx: HttpContext) {
    const { request, response } = ctx
    const payload = await request.validateUsing(createConnectionValidator)

    // Criar a conexão
    const connection = new Connection()
    connection.name = payload.name
    connection.type = payload.type
    connection.host = payload.host
    connection.port = payload.port
    connection.username = payload.username
    connection.setPassword(payload.password)
    connection.storageDestinationId = payload.storageDestinationId ?? null
    connection.scheduleFrequency = payload.scheduleFrequency ?? null
    connection.scheduleEnabled = payload.scheduleEnabled ?? false
    connection.options = payload.options ?? null
    connection.status = 'active'

    await connection.save()

    // Criar os registros de databases
    const databaseRecords: ConnectionDatabase[] = []
    for (const dbName of payload.databases) {
      const connDb = new ConnectionDatabase()
      connDb.connectionId = connection.id
      connDb.databaseName = dbName
      connDb.enabled = true
      await connDb.save()
      databaseRecords.push(connDb)
    }

    // Carregar databases para serialização
    await connection.load('databases')

    // Registrar auditoria
    await AuditService.logConnectionCreated(connection.id, connection.name, ctx)

    return response.created({
      success: true,
      message: `Conexão criada com sucesso com ${payload.databases.length} database(s)`,
      data: connection.serialize({
        relations: {
          databases: {
            fields: ['id', 'databaseName', 'enabled'],
          },
        },
      }),
    })
  }

  /**
   * GET /api/connections/:id
   * Retorna uma conexão específica com seus databases
   */
  async show({ params, response }: HttpContext) {
    const connection = await Connection.query()
      .where('id', params.id)
      .preload('databases', (dbQuery) => {
        dbQuery.orderBy('databaseName', 'asc')
      })
      .preload('backups', (query) => {
        query.orderBy('createdAt', 'desc').limit(10)
      })
      .first()

    if (!connection) {
      return response.notFound({
        success: false,
        message: 'Conexão não encontrada',
      })
    }

    return response.ok({
      success: true,
      data: connection.serialize({
        relations: {
          databases: {
            fields: ['id', 'databaseName', 'enabled'],
          },
          backups: {
            fields: [
              'id',
              'status',
              'fileName',
              'fileSize',
              'databaseName',
              'retentionType',
              'trigger',
              'createdAt',
              'finishedAt',
              'durationSeconds',
            ],
          },
        },
      }),
    })
  }

  /**
   * PUT /api/connections/:id
   * Atualiza uma conexão existente
   */
  async update(ctx: HttpContext) {
    const { params, request, response } = ctx
    const connection = await Connection.find(params.id)

    if (!connection) {
      return response.notFound({
        success: false,
        message: 'Conexão não encontrada',
      })
    }

    const payload = await request.validateUsing(updateConnectionValidator)

    // Capturar alterações para auditoria (campos não sensíveis)
    const changes: Record<string, { from: unknown; to: unknown }> = {}

    // Atualiza apenas os campos fornecidos
    if (payload.name !== undefined && payload.name !== connection.name) {
      changes.name = { from: connection.name, to: payload.name }
      connection.name = payload.name
    }
    if (payload.type !== undefined && payload.type !== connection.type) {
      changes.type = { from: connection.type, to: payload.type }
      connection.type = payload.type
    }
    if (payload.host !== undefined && payload.host !== connection.host) {
      changes.host = { from: connection.host, to: payload.host }
      connection.host = payload.host
    }
    if (payload.port !== undefined && payload.port !== connection.port) {
      changes.port = { from: connection.port, to: payload.port }
      connection.port = payload.port
    }
    if (payload.username !== undefined && payload.username !== connection.username) {
      changes.username = { from: connection.username, to: payload.username }
      connection.username = payload.username
    }
    if (payload.password !== undefined) {
      changes.password = { from: '***', to: '***' } // Não logar senha
      connection.setPassword(payload.password)
    }
    if (
      payload.storageDestinationId !== undefined &&
      payload.storageDestinationId !== connection.storageDestinationId
    ) {
      changes.storageDestinationId = {
        from: connection.storageDestinationId,
        to: payload.storageDestinationId,
      }
      connection.storageDestinationId = payload.storageDestinationId
    }
    if (
      payload.scheduleFrequency !== undefined &&
      payload.scheduleFrequency !== connection.scheduleFrequency
    ) {
      changes.scheduleFrequency = {
        from: connection.scheduleFrequency,
        to: payload.scheduleFrequency,
      }
      connection.scheduleFrequency = payload.scheduleFrequency
    }
    if (
      payload.scheduleEnabled !== undefined &&
      payload.scheduleEnabled !== connection.scheduleEnabled
    ) {
      changes.scheduleEnabled = { from: connection.scheduleEnabled, to: payload.scheduleEnabled }
      connection.scheduleEnabled = payload.scheduleEnabled
    }
    if (payload.options !== undefined) {
      connection.options = payload.options
    }

    await connection.save()

    // Atualizar databases se fornecidos
    if (payload.databases !== undefined) {
      // Obter databases atuais
      const currentDatabases = await ConnectionDatabase.query()
        .where('connectionId', connection.id)
        .select('databaseName')

      const currentDbNames = new Set(currentDatabases.map((d) => d.databaseName))
      const newDbNames = new Set(payload.databases)

      // Databases para adicionar
      const toAdd = payload.databases.filter((db) => !currentDbNames.has(db))

      // Databases para remover (desabilitar)
      const toRemove = [...currentDbNames].filter((db) => !newDbNames.has(db))

      // Adicionar novos databases
      for (const dbName of toAdd) {
        const connDb = new ConnectionDatabase()
        connDb.connectionId = connection.id
        connDb.databaseName = dbName
        connDb.enabled = true
        await connDb.save()
      }

      // Desabilitar databases removidos (não deletar para manter histórico de backups)
      if (toRemove.length > 0) {
        await ConnectionDatabase.query()
          .where('connectionId', connection.id)
          .whereIn('databaseName', toRemove)
          .update({ enabled: false })
      }

      // Reabilitar databases que foram readicionados
      const toReactivate = payload.databases.filter((db) => currentDbNames.has(db))
      if (toReactivate.length > 0) {
        await ConnectionDatabase.query()
          .where('connectionId', connection.id)
          .whereIn('databaseName', toReactivate)
          .update({ enabled: true })
      }

      changes.databases = { from: [...currentDbNames], to: payload.databases }
    }

    // Registrar auditoria
    if (Object.keys(changes).length > 0) {
      await AuditService.logConnectionUpdated(connection.id, connection.name, changes, ctx)
    }

    // Recarregar databases
    await connection.load('databases', (query) => {
      query.where('enabled', true).orderBy('databaseName', 'asc')
    })

    return response.ok({
      success: true,
      message: 'Conexão atualizada com sucesso',
      data: connection.serialize({
        relations: {
          databases: {
            fields: ['id', 'databaseName', 'enabled'],
          },
        },
      }),
    })
  }

  /**
   * DELETE /api/connections/:id
   * Remove uma conexão e seus databases/backups associados
   */
  async destroy(ctx: HttpContext) {
    const { params, response } = ctx
    const connection = await Connection.find(params.id)

    if (!connection) {
      return response.notFound({
        success: false,
        message: 'Conexão não encontrada',
      })
    }

    const connectionName = connection.name
    const connectionId = connection.id

    // TODO: Deletar arquivos de backup físicos antes de remover do banco
    await connection.delete()

    // Registrar auditoria
    await AuditService.logConnectionDeleted(connectionId, connectionName, ctx)

    return response.ok({
      success: true,
      message: 'Conexão removida com sucesso',
    })
  }

  /**
   * POST /api/connections/:id/test
   * Testa a conexão com o banco de dados remoto
   */
  async test(ctx: HttpContext) {
    const { params, response } = ctx
    const connection = await Connection.find(params.id)

    if (!connection) {
      return response.notFound({
        success: false,
        message: 'Conexão não encontrada',
      })
    }

    try {
      const testResult = await this.performConnectionTest(connection)

      // Atualiza o status da conexão
      connection.status = testResult.success ? 'active' : 'error'
      connection.lastError = testResult.success ? null : (testResult.error ?? null)
      connection.lastTestedAt = DateTime.now()

      await connection.save()

      // Registrar auditoria
      await AuditService.logConnectionTested(
        connection.id,
        connection.name,
        testResult.success,
        testResult.error,
        ctx
      )

      if (testResult.success) {
        // Notificar sucesso do teste
        NotificationService.connectionTestSuccess(connection.name, connection.id)

        return response.ok({
          success: true,
          message: 'Conexão testada com sucesso',
          data: {
            latencyMs: testResult.latencyMs,
            version: testResult.version,
          },
        })
      } else {
        // Notificar falha do teste
        NotificationService.connectionTestFailed(
          connection.name,
          connection.id,
          testResult.error ?? 'Erro desconhecido'
        )

        return response.unprocessableEntity({
          success: false,
          message: 'Falha ao conectar ao banco de dados',
          error: testResult.error,
        })
      }
    } catch (error) {
      connection.status = 'error'
      connection.lastError = error instanceof Error ? error.message : 'Erro desconhecido'
      connection.lastTestedAt = DateTime.now()
      await connection.save()

      // Registrar auditoria de falha
      await AuditService.logConnectionTested(
        connection.id,
        connection.name,
        false,
        error instanceof Error ? error.message : 'Erro desconhecido',
        ctx
      )

      return response.internalServerError({
        success: false,
        message: 'Erro ao testar conexão',
        error: error instanceof Error ? error.message : 'Erro desconhecido',
      })
    }
  }

  /**
   * Realiza o teste de conexão com o banco de dados
   */
  private async performConnectionTest(
    connection: Connection
  ): Promise<{ success: boolean; latencyMs?: number; version?: string; error?: string }> {
    const password = connection.getDecryptedPassword()
    const startTime = Date.now()

    // Obter primeiro database habilitado para testar, ou usar 'postgres'/'mysql' como padrão
    const databases = await connection.getEnabledDatabases()
    const testDatabase =
      databases.length > 0
        ? databases[0].databaseName
        : connection.type === 'postgresql'
          ? 'postgres'
          : 'mysql'

    try {
      // Importação dinâmica para evitar carregar drivers desnecessários
      if (connection.type === 'postgresql') {
        const { default: pg } = await import('pg')
        const client = new pg.Client({
          host: connection.host,
          port: connection.port,
          database: testDatabase,
          user: connection.username,
          password: password,
          connectionTimeoutMillis: 10000,
        })

        await client.connect()
        const result = await client.query('SELECT version() as version')
        await client.end()

        return {
          success: true,
          latencyMs: Date.now() - startTime,
          version: result.rows[0]?.version,
        }
      } else {
        // MySQL / MariaDB
        const mysql = await import('mysql2/promise')
        const conn = await mysql.createConnection({
          host: connection.host,
          port: connection.port,
          database: testDatabase,
          user: connection.username,
          password: password,
          connectTimeout: 10000,
        })

        const [rows] = await conn.query('SELECT VERSION() as version')
        await conn.end()

        const version =
          Array.isArray(rows) && rows[0] ? (rows[0] as { version: string }).version : undefined

        return {
          success: true,
          latencyMs: Date.now() - startTime,
          version,
        }
      }
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Erro de conexão desconhecido',
      }
    }
  }

  /**
   * POST /api/connections/:id/backup
   * Inicia um backup manual de TODOS os databases da conexão
   */
  async backup(ctx: HttpContext) {
    const { params, response } = ctx
    const connection = await Connection.find(params.id)

    if (!connection) {
      return response.notFound({
        success: false,
        message: 'Conexão não encontrada',
      })
    }

    if (connection.status === 'error') {
      return response.unprocessableEntity({
        success: false,
        message: 'Não é possível fazer backup de uma conexão com erro. Teste a conexão primeiro.',
      })
    }

    // Verificar se há databases habilitados
    const databaseCount = await connection.getEnabledDatabasesCount()
    if (databaseCount === 0) {
      return response.unprocessableEntity({
        success: false,
        message: 'Nenhum database habilitado para backup nesta conexão.',
      })
    }

    try {
      const backupService = new BackupService()
      const result = await backupService.executeAll(connection, 'manual')

      // Registrar auditoria de início para cada backup
      for (const r of result.results) {
        await AuditService.logBackupStarted(r.backup.id, connection.name, 'manual', ctx)

        if (r.result.success) {
          await AuditService.logBackupCompleted(
            r.backup.id,
            `${connection.name} / ${r.databaseName}`,
            r.backup.fileSize ?? 0,
            r.backup.durationSeconds ?? 0,
            ctx
          )
        } else {
          await AuditService.logBackupFailed(
            r.backup.id,
            `${connection.name} / ${r.databaseName}`,
            r.result.error ?? 'Erro desconhecido',
            ctx
          )
        }
      }

      if (result.failed === 0) {
        return response.ok({
          success: true,
          message: `Backup realizado com sucesso para ${result.successful} database(s)`,
          data: {
            totalDatabases: result.totalDatabases,
            successful: result.successful,
            failed: result.failed,
            backups: result.results.map((r) => ({
              databaseName: r.databaseName,
              backupId: r.backup.id,
              fileName: r.result.fileName,
              fileSize: r.backup.getFormattedSize(),
              duration: r.backup.getFormattedDuration(),
            })),
          },
        })
      } else {
        return response.unprocessableEntity({
          success: false,
          message: `Backup parcialmente concluído: ${result.successful} sucesso, ${result.failed} falha(s)`,
          data: {
            totalDatabases: result.totalDatabases,
            successful: result.successful,
            failed: result.failed,
            backups: result.results.map((r) => ({
              databaseName: r.databaseName,
              backupId: r.backup.id,
              success: r.result.success,
              error: r.result.error,
            })),
          },
        })
      }
    } catch (error) {
      return response.internalServerError({
        success: false,
        message: 'Erro ao executar backup',
        error: error instanceof Error ? error.message : 'Erro desconhecido',
      })
    }
  }

  /**
   * POST /api/connections/discover-databases
   * Descobre os bancos de dados disponíveis com as credenciais fornecidas
   */
  async discoverDatabases({ request, response }: HttpContext) {
    const payload = await request.validateUsing(discoverDatabasesValidator)

    try {
      const databases = await this.listAvailableDatabases(
        payload.type,
        payload.host,
        payload.port,
        payload.username,
        payload.password ?? ''
      )

      return response.ok({
        success: true,
        message: 'Bancos de dados descobertos com sucesso',
        data: {
          databases,
        },
      })
    } catch (error) {
      return response.unprocessableEntity({
        success: false,
        message: 'Falha ao conectar ao servidor de banco de dados',
        error: error instanceof Error ? error.message : 'Erro de conexão desconhecido',
      })
    }
  }

  /**
   * Lista os bancos de dados disponíveis no servidor
   */
  private async listAvailableDatabases(
    type: 'mysql' | 'mariadb' | 'postgresql',
    host: string,
    port: number,
    username: string,
    password: string
  ): Promise<string[]> {
    if (type === 'postgresql') {
      const { default: pg } = await import('pg')
      const client = new pg.Client({
        host,
        port,
        database: 'postgres', // Conecta ao banco padrão para listar os outros
        user: username,
        password,
        connectionTimeoutMillis: 10000,
      })

      await client.connect()
      const result = await client.query(`
        SELECT datname 
        FROM pg_database 
        WHERE datistemplate = false 
          AND datallowconn = true
          AND datname NOT IN ('postgres')
        ORDER BY datname
      `)
      await client.end()

      return result.rows.map((row: { datname: string }) => row.datname)
    } else {
      // MySQL / MariaDB
      const mysql = await import('mysql2/promise')
      const conn = await mysql.createConnection({
        host,
        port,
        user: username,
        password,
        connectTimeout: 10000,
      })

      const [rows] = await conn.query(`
        SHOW DATABASES 
        WHERE \`Database\` NOT IN ('information_schema', 'mysql', 'performance_schema', 'sys')
      `)
      await conn.end()

      return (rows as { Database: string }[]).map((row) => row.Database)
    }
  }
}
