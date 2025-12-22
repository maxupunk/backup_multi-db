import type { HttpContext } from '@adonisjs/core/http'
import { DateTime } from 'luxon'
import Connection from '#models/connection'
import {
  createConnectionValidator,
  updateConnectionValidator,
  listConnectionsValidator,
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

    // Busca por nome, host ou database
    if (query.search) {
      const searchTerm = `%${query.search}%`
      connectionsQuery.where((builder) => {
        builder
          .whereILike('name', searchTerm)
          .orWhereILike('host', searchTerm)
          .orWhereILike('database', searchTerm)
      })
    }

    // Carregar último backup de cada conexão
    connectionsQuery.preload('backups', (backupsQuery) => {
      backupsQuery.orderBy('createdAt', 'desc').limit(1)
    })

    const connections = await connectionsQuery.paginate(page, limit)

    return response.ok({
      success: true,
      data: connections.serialize({
        relations: {
          backups: {
            fields: ['id', 'status', 'fileSize', 'createdAt', 'finishedAt'],
          },
        },
      }),
    })
  }

  /**
   * POST /api/connections
   * Cria uma nova conexão
   */
  async store(ctx: HttpContext) {
    const { request, response } = ctx
    const payload = await request.validateUsing(createConnectionValidator)

    const connection = new Connection()
    connection.name = payload.name
    connection.type = payload.type
    connection.host = payload.host
    connection.port = payload.port
    connection.database = payload.database
    connection.username = payload.username
    connection.setPassword(payload.password)
    connection.storageDestinationId = payload.storageDestinationId ?? null
    connection.scheduleFrequency = payload.scheduleFrequency ?? null
    connection.scheduleEnabled = payload.scheduleEnabled ?? false
    connection.options = payload.options ?? null
    connection.status = 'active'

    await connection.save()

    // Registrar auditoria
    await AuditService.logConnectionCreated(connection.id, connection.name, ctx)

    return response.created({
      success: true,
      message: 'Conexão criada com sucesso',
      data: connection,
    })
  }

  /**
   * GET /api/connections/:id
   * Retorna uma conexão específica
   */
  async show({ params, response }: HttpContext) {
    const connection = await Connection.query()
      .where('id', params.id)
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
          backups: {
            fields: [
              'id',
              'status',
              'fileName',
              'fileSize',
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
    if (payload.database !== undefined && payload.database !== connection.database) {
      changes.database = { from: connection.database, to: payload.database }
      connection.database = payload.database
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

    // Registrar auditoria
    if (Object.keys(changes).length > 0) {
      await AuditService.logConnectionUpdated(connection.id, connection.name, changes, ctx)
    }

    return response.ok({
      success: true,
      message: 'Conexão atualizada com sucesso',
      data: connection,
    })
  }

  /**
   * DELETE /api/connections/:id
   * Remove uma conexão e seus backups associados
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

    try {
      // Importação dinâmica para evitar carregar drivers desnecessários
      if (connection.type === 'postgresql') {
        const { default: pg } = await import('pg')
        const client = new pg.Client({
          host: connection.host,
          port: connection.port,
          database: connection.database,
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
          database: connection.database,
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
   * Inicia um backup manual da conexão
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

    try {
      const backupService = new BackupService()
      const { backup, result } = await backupService.execute(connection, 'manual')

      // Registrar auditoria de início
      await AuditService.logBackupStarted(backup.id, connection.name, 'manual', ctx)

      if (result.success) {
        // Registrar auditoria de sucesso
        await AuditService.logBackupCompleted(
          backup.id,
          connection.name,
          backup.fileSize ?? 0,
          backup.durationSeconds ?? 0,
          ctx
        )

        return response.ok({
          success: true,
          message: 'Backup realizado com sucesso',
          data: {
            backupId: backup.id,
            fileName: result.fileName,
            fileSize: backup.getFormattedSize(),
            duration: backup.getFormattedDuration(),
            checksum: result.checksum,
          },
        })
      } else {
        // Registrar auditoria de falha
        await AuditService.logBackupFailed(
          backup.id,
          connection.name,
          result.error ?? 'Erro desconhecido',
          ctx
        )

        return response.unprocessableEntity({
          success: false,
          message: 'Falha ao realizar backup',
          error: result.error,
          data: {
            backupId: backup.id,
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
}
