import { createWriteStream } from 'node:fs'
import { unlink } from 'node:fs/promises'
import { join } from 'node:path'
import { pipeline } from 'node:stream/promises'
import { createGzip } from 'node:zlib'
import type { HttpContext } from '@adonisjs/core/http'
import {
  DockerManagerService,
  VolumeInUseError,
  ImageInUseError,
} from '#services/docker_manager_service'
import { DockerEngineHttpClient } from '#services/docker_engine_http_client'
import StorageDestination from '#models/storage_destination'
import { StorageDestinationService } from '#services/storage_destination_service'

const UNAVAILABLE = { success: true, available: false, data: [] } as const

export default class DockerManagerController {
  private readonly service: DockerManagerService

  constructor() {
    this.service = new DockerManagerService(new DockerEngineHttpClient())
  }

  // ================================================================
  // Containers
  // ================================================================

  /**
   * GET /api/docker/containers
   * Lista todos os containers agrupados por projeto docker-compose.
   */
  async listContainers({ response }: HttpContext) {
    if (!this.service.isAvailable()) return response.ok(UNAVAILABLE)
    const groups = await this.service.listContainers()
    return response.ok({ success: true, available: true, data: groups })
  }

  /**
   * GET /api/docker/containers/:id
   * Retorna detalhes completos de um container.
   */
  async inspectContainer({ params, response }: HttpContext) {
    const detail = await this.service.inspectContainer(params.id as string)
    return response.ok({ success: true, data: detail })
  }

  /**
   * POST /api/docker/containers/:id/start
   */
  async startContainer({ params, response }: HttpContext) {
    const result = await this.service.startContainer(params.id as string)
    return response.ok({ success: true, data: result })
  }

  /**
   * POST /api/docker/containers/:id/stop
   */
  async stopContainer({ params, response }: HttpContext) {
    const result = await this.service.stopContainer(params.id as string)
    return response.ok({ success: true, data: result })
  }

  /**
   * POST /api/docker/containers/:id/restart
   */
  async restartContainer({ params, response }: HttpContext) {
    const result = await this.service.restartContainer(params.id as string)
    return response.ok({ success: true, data: result })
  }

  /**
   * GET /api/docker/containers/:id/logs
   * Query params: tail (number|'all'), since (unix timestamp), timestamps (bool)
   */
  async containerLogs({ params, request, response }: HttpContext) {
    const tailParam = request.input('tail', '200')
    const sinceParam = request.input('since')
    const timestampsParam = request.input('timestamps', 'false')

    const tail = tailParam === 'all' ? ('all' as const) : Number(tailParam) || 200
    const since = sinceParam ? Number(sinceParam) : undefined
    const timestamps = timestampsParam === 'true' || timestampsParam === '1'

    const entries = await this.service.getContainerLogs(params.id as string, {
      tail,
      since,
      timestamps,
    })

    return response.ok({ success: true, data: entries })
  }

  /**
   * DELETE /api/docker/containers/:id
   * Remove um container. Query: force=true
   */
  async removeContainer({ params, request, response }: HttpContext) {
    const force = request.input('force', 'false') === 'true'
    const result = await this.service.removeContainer(params.id as string, force)
    return response.ok({ success: true, data: result })
  }

  // ================================================================
  // Volumes
  // ================================================================

  /**
   * GET /api/docker/volumes
   */
  async listVolumes({ response }: HttpContext) {
    if (!this.service.isAvailable()) return response.ok(UNAVAILABLE)
    const volumes = await this.service.listVolumes()
    return response.ok({ success: true, available: true, data: volumes })
  }

  /**
   * GET /api/docker/volumes/:name
   */
  async inspectVolume({ params, response }: HttpContext) {
    const detail = await this.service.inspectVolume(params.name as string)
    return response.ok({ success: true, data: detail })
  }

  /**
   * DELETE /api/docker/volumes/:name
   * Query: force=true
   */
  async removeVolume({ params, request, response }: HttpContext) {
    const force = request.input('force', 'false') === 'true'
    try {
      const result = await this.service.removeVolume(params.name as string, force)
      return response.ok({ success: true, data: result })
    } catch (error) {
      if (error instanceof VolumeInUseError) {
        return response.conflict({ message: error.message })
      }
      throw error
    }
  }

  /**
   * POST /api/docker/volumes/:name/backup
   * Faz backup do volume para um destino de armazenamento.
   * Body: { storageId: number }
   */
  async backupVolumeToStorage({ params, request, response }: HttpContext) {
    const name = params.name as string
    const storageId = Number(request.input('storageId'))

    if (!storageId) {
      return response.badRequest({ success: false, message: 'storageId é obrigatório' })
    }

    const destination = await StorageDestination.find(storageId)
    if (!destination) {
      return response.notFound({
        success: false,
        message: 'Destino de armazenamento não encontrado',
      })
    }

    const safeName = name.replace(/[^a-zA-Z0-9_-]/g, '_')
    const date = new Date().toISOString().slice(0, 10)
    const fileName = `volume-${safeName}-${date}.tar.gz`
    const relativePath = join('docker-volumes', fileName)

    const localBasePath = StorageDestinationService.getLocalBasePath(destination)
    const localFullPath = join(localBasePath, relativePath)
    StorageDestinationService.ensureLocalDirectory(localFullPath)

    const { stream, cleanup } = await this.service.exportVolumeAsArchive(name)
    try {
      const gzip = createGzip()
      await pipeline(stream, gzip, createWriteStream(localFullPath))
      await cleanup()

      const config = destination.getDecryptedConfig()
      if (config && config.type !== 'local') {
        await StorageDestinationService.uploadBackupFile(destination, relativePath, localFullPath)
        await unlink(localFullPath)
      }

      return response.ok({ success: true, data: { fileName, relativePath } })
    } catch (error) {
      await cleanup()
      throw error
    }
  }

  /**
   * GET /api/docker/volumes/:name/export
   * Exporta o conteúdo do volume como arquivo tar.gz para download.
   */
  async exportVolume({ params, response }: HttpContext) {
    const name = params.name as string
    const { stream, cleanup } = await this.service.exportVolumeAsArchive(name)

    const safeName = name.replace(/[^a-zA-Z0-9_-]/g, '_')
    const date = new Date().toISOString().slice(0, 10)
    const fileName = `volume-${safeName}-${date}.tar.gz`

    response.header('Content-Type', 'application/gzip')
    response.header('Content-Disposition', `attachment; filename="${fileName}"`)

    const gzip = createGzip()

    // Cleanup do container temporário após o stream encerrar
    gzip.on('close', () => {
      void cleanup()
    })

    stream.pipe(gzip)
    return response.stream(gzip)
  }

  // ================================================================
  // Networks
  // ================================================================

  /**
   * GET /api/docker/networks
   */
  async listNetworks({ response }: HttpContext) {
    if (!this.service.isAvailable()) return response.ok(UNAVAILABLE)
    const networks = await this.service.listNetworks()
    return response.ok({ success: true, available: true, data: networks })
  }

  /**
   * GET /api/docker/networks/:id
   */
  async inspectNetwork({ params, response }: HttpContext) {
    const detail = await this.service.inspectNetwork(params.id as string)
    return response.ok({ success: true, data: detail })
  }

  /**
   * POST /api/docker/networks
   * Cria uma nova rede Docker.
   */
  async createNetwork({ request, response }: HttpContext) {
    const name = request.input('name') as string | undefined
    const driver = (request.input('driver') as string | undefined) ?? 'bridge'

    if (!name || typeof name !== 'string' || !name.trim()) {
      return response.badRequest({ success: false, message: 'Nome da rede é obrigatório.' })
    }

    const result = await this.service.createNetwork(name.trim(), driver)
    return response.ok({ success: true, data: result })
  }

  /**
   * POST /api/docker/networks/:id/connect
   * Conecta um container a uma rede.
   */
  async connectContainerToNetwork({ params, request, response }: HttpContext) {
    const containerId = request.input('containerId') as string | undefined

    if (!containerId || typeof containerId !== 'string') {
      return response.badRequest({ success: false, message: 'containerId é obrigatório.' })
    }

    const result = await this.service.connectContainerToNetwork(containerId, params.id as string)
    return response.ok({ success: true, data: result })
  }

  /**
   * POST /api/docker/networks/:id/disconnect
   * Desconecta um container de uma rede.
   */
  async disconnectContainerFromNetwork({ params, request, response }: HttpContext) {
    const containerId = request.input('containerId') as string | undefined
    const force = request.input('force', false) === true || request.input('force') === 'true'

    if (!containerId || typeof containerId !== 'string') {
      return response.badRequest({ success: false, message: 'containerId é obrigatório.' })
    }

    const result = await this.service.disconnectContainerFromNetwork(
      containerId,
      params.id as string,
      force
    )
    return response.ok({ success: true, data: result })
  }

  // ================================================================
  // Images
  // ================================================================

  /**
   * GET /api/docker/images
   */
  async listImages({ response }: HttpContext) {
    if (!this.service.isAvailable()) return response.ok(UNAVAILABLE)
    const images = await this.service.listImages()
    return response.ok({ success: true, available: true, data: images })
  }

  /**
   * GET /api/docker/images/:id
   */
  async inspectImage({ params, response }: HttpContext) {
    const detail = await this.service.inspectImage(params.id as string)
    return response.ok({ success: true, data: detail })
  }

  /**
   * DELETE /api/docker/images/:id
   * Query: force=true
   */
  async removeImage({ params, request, response }: HttpContext) {
    const force = request.input('force', 'false') === 'true'
    try {
      const result = await this.service.removeImage(params.id as string, force)
      return response.ok({ success: true, data: result })
    } catch (error) {
      if (error instanceof ImageInUseError) {
        return response.conflict({ message: error.message })
      }
      throw error
    }
  }

  /**
   * POST /api/docker/images/prune
   */
  async pruneImages({ response }: HttpContext) {
    const result = await this.service.pruneImages()
    return response.ok({ success: true, data: result })
  }
}
