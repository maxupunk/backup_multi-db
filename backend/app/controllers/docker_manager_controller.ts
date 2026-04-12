import type { HttpContext } from '@adonisjs/core/http'
import { DockerManagerService } from '#services/docker_manager_service'
import { DockerEngineHttpClient } from '#services/docker_engine_http_client'

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
    const result = await this.service.removeVolume(params.name as string, force)
    return response.ok({ success: true, data: result })
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
    const result = await this.service.removeImage(params.id as string, force)
    return response.ok({ success: true, data: result })
  }

  /**
   * POST /api/docker/images/prune
   */
  async pruneImages({ response }: HttpContext) {
    const result = await this.service.pruneImages()
    return response.ok({ success: true, data: result })
  }
}
