import type { HttpContext } from '@adonisjs/core/http'
import Backup from '#models/backup'
import Connection from '#models/connection'
import { createReadStream, existsSync } from 'node:fs'
import { AuditService } from '#services/audit_service'
import { StorageDestinationService } from '#services/storage_destination_service'
import { RestoreService, type RestoreOptions } from '#services/restore_service'
import { RestoreProgressEmitter } from '#services/restore_progress_emitter'
import { BackupImportService } from '#services/backup_import_service'
import { restoreBackupValidator } from '#validators/restore_validator'
import { importBackupValidator } from '#validators/import_validator'
import logger from '@adonisjs/core/services/logger'
import type { Readable } from 'node:stream'

/**
 * Controller para gerenciamento de backups
 */
export default class BackupsController {
  /**
   * GET /api/backups
   * Lista todos os backups com paginação
   */
  async index({ request, response }: HttpContext) {
    const page = request.input('page', 1)
    const limit = request.input('limit', 20)
    const status = request.input('status')
    const connectionId = request.input('connectionId')

    const query = Backup.query().preload('connection').orderBy('createdAt', 'desc')

    if (status) {
      query.where('status', status)
    }

    if (connectionId) {
      query.where('connectionId', connectionId)
    }

    const backups = await query.paginate(page, limit)

    return response.ok({
      success: true,
      data: backups.serialize({
        relations: {
          connection: {
            fields: ['id', 'name', 'type', 'host', 'database'],
          },
        },
      }),
    })
  }

  /**
   * GET /api/connections/:connectionId/backups
   * Lista backups de uma conexão específica
   */
  async byConnection({ params, request, response }: HttpContext) {
    const page = request.input('page', 1)
    const limit = request.input('limit', 20)

    const connection = await Connection.find(params.connectionId)

    if (!connection) {
      return response.notFound({
        success: false,
        message: 'Conexão não encontrada',
      })
    }

    const backups = await Backup.query()
      .where('connectionId', params.connectionId)
      .orderBy('createdAt', 'desc')
      .paginate(page, limit)

    return response.ok({
      success: true,
      data: backups,
    })
  }

  /**
   * GET /api/backups/:id
   * Retorna detalhes de um backup específico
   */
  async show({ params, response }: HttpContext) {
    const backup = await Backup.query().where('id', params.id).preload('connection').first()

    if (!backup) {
      return response.notFound({
        success: false,
        message: 'Backup não encontrado',
      })
    }

    return response.ok({
      success: true,
      data: backup.serialize({
        relations: {
          connection: {
            fields: ['id', 'name', 'type', 'host', 'database'],
          },
        },
      }),
    })
  }

  /**
   * GET /api/backups/:id/download
   * Faz download do arquivo de backup
   */
  async download(ctx: HttpContext) {
    const { params, response } = ctx
    const backup = await Backup.query()
      .where('id', params.id)
      .preload('connection')
      .preload('storageDestination')
      .first()

    if (!backup) {
      return response.notFound({
        success: false,
        message: 'Backup não encontrado',
      })
    }

    if (!backup.filePath || !backup.fileName) {
      return response.notFound({
        success: false,
        message: 'Arquivo de backup não disponível',
      })
    }

    const destination = backup.storageDestination ?? null
    const fullPath = StorageDestinationService.getLocalFullPath(destination, backup.filePath)

    try {
      let stream: Readable
      let contentLength: number | undefined = backup.fileSize ?? undefined

      if (existsSync(fullPath)) {
        stream = createReadStream(fullPath)
      } else if (destination) {
        const download = await StorageDestinationService.getDownloadStream(destination, backup.filePath)
        stream = download.stream
        contentLength = download.contentLength ?? contentLength
      } else {
        return response.notFound({
          success: false,
          message: 'Arquivo de backup não encontrado no servidor',
        })
      }

      await AuditService.logBackupDownloaded(backup.id, backup.connection?.name ?? 'N/A', ctx)

      response.header('Content-Type', 'application/octet-stream')
      response.header('Content-Disposition', `attachment; filename="${backup.fileName}"`)

      if (contentLength) {
        response.header('Content-Length', contentLength.toString())
      }

      return response.stream(stream)
    } catch (error) {
      return response.internalServerError({
        success: false,
        message: 'Erro ao fazer download do backup',
        error: error instanceof Error ? error.message : 'Erro desconhecido',
      })
    }
  }

  /**
   * DELETE /api/backups/:id
   * Remove um backup
   */
  async destroy(ctx: HttpContext) {
    const { params, response } = ctx
    const backup = await Backup.query()
      .where('id', params.id)
      .preload('connection')
      .preload('storageDestination')
      .first()

    if (!backup) {
      return response.notFound({
        success: false,
        message: 'Backup não encontrado',
      })
    }

    if (!backup.canBeDeleted()) {
      return response.unprocessableEntity({
        success: false,
        message: 'Este backup não pode ser deletado (protegido ou em execução)',
      })
    }

    const connectionName = backup.connection?.name ?? 'N/A'
    const backupId = backup.id

    // Deletar arquivo físico se existir
    if (backup.filePath) {
      try {
        await StorageDestinationService.deleteBackupFile(
          backup.storageDestination ?? null,
          backup.filePath
        )
      } catch (error) {
        console.error('Erro ao deletar arquivo de backup:', error)
      }
    }

    await backup.delete()

    // Registrar auditoria
    await AuditService.logBackupDeleted(backupId, connectionName, ctx)

    return response.ok({
      success: true,
      message: 'Backup removido com sucesso',
    })
  }

  /**
   * POST /api/backups/:id/restore
   * Inicia a restauração de um backup de forma assíncrona.
   * Retorna 202 (Accepted) imediatamente com o restoreId.
   * O progresso é acompanhado via SSE no canal notifications/restore.
   */
  async restore(ctx: HttpContext) {
    const { params, request, response } = ctx

    // Validar payload
    const payload = await request.validateUsing(restoreBackupValidator)

    // Buscar backup com conexão
    const backup = await Backup.query()
      .where('id', params.id)
      .preload('connection')
      .first()

    if (!backup) {
      return response.notFound({
        success: false,
        message: 'Backup não encontrado',
      })
    }

    if (backup.status !== 'completed') {
      return response.unprocessableEntity({
        success: false,
        message: 'Apenas backups concluídos podem ser restaurados',
      })
    }

    if (!backup.filePath || !backup.fileName) {
      return response.unprocessableEntity({
        success: false,
        message: 'Arquivo de backup não disponível',
      })
    }

    if (!backup.connection) {
      return response.unprocessableEntity({
        success: false,
        message: 'Conexão associada ao backup não encontrada',
      })
    }

    // Resolver conexão de destino (pode ser diferente da origem)
    // Cast necessário: Lucid declara o campo como BelongsTo<T>, mas após preload o valor é a instância da conexão
    let targetConnection = backup.connection as unknown as Connection

    if (payload.targetConnectionId && payload.targetConnectionId !== backup.connectionId) {
      const specifiedConnection = await Connection.find(payload.targetConnectionId)

      if (!specifiedConnection) {
        return response.notFound({
          success: false,
          message: 'Conexão de destino não encontrada',
        })
      }

      if (specifiedConnection.type !== backup.connection.type) {
        return response.unprocessableEntity({
          success: false,
          message: `O tipo da conexão de destino (${specifiedConnection.type}) deve ser igual ao da conexão original do backup (${backup.connection.type})`,
        })
      }

      targetConnection = specifiedConnection
    }

    const options: RestoreOptions = {
      mode: payload.mode ?? 'full',
      targetDatabase: payload.targetDatabase,
      noOwner: payload.noOwner,
      noPrivileges: payload.noPrivileges,
      noTablespaces: payload.noTablespaces,
      noComments: payload.noComments,
      noCreateDb: payload.noCreateDb,
      skipSafetyBackup: payload.skipSafetyBackup,
    }

    const targetDb = options.targetDatabase || backup.databaseName

    // Criar emissor de progresso
    const emitter = new RestoreProgressEmitter(
      backup.id,
      targetDb,
      targetConnection.name
    )

    // Notificar início (toast + progresso SSE)
    emitter.started()

    // Executar restauração em background (não aguarda)
    const restoreService = new RestoreService()
    restoreService
      .restore(backup, targetConnection, options, emitter)
      .catch((error) => {
        const errorMessage = error instanceof Error ? error.message : 'Erro desconhecido'
        logger.error(`[Restore] Erro não tratado na restauração em background: ${errorMessage}`)
        emitter.failed(errorMessage)
      })

    // Retorna imediatamente — progresso é acompanhado via SSE
    return response.accepted({
      success: true,
      message: 'Restauração iniciada com sucesso. Acompanhe o progresso em tempo real.',
      data: {
        restoreId: emitter.getRestoreId(),
        databaseName: targetDb,
      },
    })
  }

  /**
   * POST /api/backups/import
   * Importa um arquivo de backup externo e o registra no sistema.
   *
   * Formatos suportados: .sql | .sql.gz | .gz | .dump | .pgdump | .zip | .tar | .tar.gz | .tgz
   *
   * Aceita multipart/form-data com:
   *   - file          : arquivo de backup
   *   - connectionId  : ID da conexão de banco
   *   - databaseName  : nome do database
   *   - verifyIntegrity (optional, default false): verifica magic bytes / SQL content
   */
  async import(ctx: HttpContext) {
    const { request, response } = ctx

    // Validar campos do formulário
    const payload = await request.validateUsing(importBackupValidator)

    // Recuperar o arquivo enviado
    const file = request.file('file', {
      size: '500mb',
      extnames: ['sql', 'gz', 'dump', 'pgdump', 'pg_dump', 'zip', 'tar', 'tgz'],
    })

    if (!file) {
      return response.unprocessableEntity({
        success: false,
        message: 'Nenhum arquivo enviado. Inclua o campo "file" no formulário multipart.',
      })
    }

    // Validar que o arquivo foi processado sem erros pelo multipart parser
    if (file.hasErrors) {
      const firstError = file.errors[0]
      return response.unprocessableEntity({
        success: false,
        message: firstError?.message ?? 'Arquivo inválido',
      })
    }

    // Verificar conexão
    const connection = await Connection.find(payload.connectionId)

    if (!connection) {
      return response.notFound({
        success: false,
        message: 'Conexão não encontrada',
      })
    }

    try {
      const importService = new BackupImportService()

      const result = await importService.import(file, connection, {
        connectionId: payload.connectionId,
        databaseName: payload.databaseName,
        verifyIntegrity: payload.verifyIntegrity ?? false,
      })

      // Auditoria
      await AuditService.logBackupImported(result.backup.id, connection.name, ctx)

      return response.created({
        success: true,
        message: 'Backup importado com sucesso',
        data: {
          backup: result.backup.serialize(),
          format: result.format,
          checksum: result.checksum,
          fileSize: result.fileSize,
          integrity: result.integrityResult
            ? {
                valid: result.integrityResult.valid,
                message: result.integrityResult.message,
                warnings: result.integrityResult.warnings,
              }
            : null,
        },
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Erro ao importar backup'
      logger.error(`[Import] ${message}`)

      return response.unprocessableEntity({
        success: false,
        message,
      })
    }
  }
}
