import type { HttpContext } from '@adonisjs/core/http'
import Backup from '#models/backup'
import Connection from '#models/connection'
import { createReadStream, existsSync } from 'node:fs'
import { unlink } from 'node:fs/promises'
import { join } from 'node:path'
import app from '@adonisjs/core/services/app'

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

    const query = Backup.query()
      .preload('connection')
      .orderBy('createdAt', 'desc')

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
  async download({ params, response }: HttpContext) {
    const backup = await Backup.find(params.id)

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

    const fullPath = join(app.makePath('storage/backups'), backup.filePath)

    if (!existsSync(fullPath)) {
      return response.notFound({
        success: false,
        message: 'Arquivo de backup não encontrado no servidor',
      })
    }

    // Stream do arquivo para download
    const stream = createReadStream(fullPath)

    response.header('Content-Type', 'application/octet-stream')
    response.header('Content-Disposition', `attachment; filename="${backup.fileName}"`)

    if (backup.fileSize) {
      response.header('Content-Length', backup.fileSize.toString())
    }

    return response.stream(stream)
  }

  /**
   * DELETE /api/backups/:id
   * Remove um backup
   */
  async destroy({ params, response }: HttpContext) {
    const backup = await Backup.find(params.id)

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

    // Deletar arquivo físico se existir
    if (backup.filePath) {
      const fullPath = join(app.makePath('storage/backups'), backup.filePath)

      if (existsSync(fullPath)) {
        try {
          await unlink(fullPath)
        } catch (error) {
          console.error('Erro ao deletar arquivo de backup:', error)
        }
      }
    }

    await backup.delete()

    return response.ok({
      success: true,
      message: 'Backup removido com sucesso',
    })
  }
}
