import type { HttpContext } from '@adonisjs/core/http'
import { DockerDiagnosticsService } from '#services/docker_diagnostics_service'
import { startDockerDiagnosticValidator } from '#validators/docker_diagnostic_validator'

export default class DockerDiagnosticsController {
  private readonly service: DockerDiagnosticsService

  constructor() {
    this.service = new DockerDiagnosticsService()
  }

  async start({ request, response }: HttpContext) {
    const payload = await request.validateUsing(startDockerDiagnosticValidator)
    try {
      const job = await this.service.start(payload)

      return response.accepted({
        success: true,
        data: job,
      })
    } catch (error) {
      return response.badRequest({
        success: false,
        message: error instanceof Error ? error.message : 'Payload de diagnóstico inválido',
      })
    }
  }

  async show({ params, response }: HttpContext) {
    const job = this.service.getJob(params.jobId as string)

    if (!job) {
      return response.notFound({
        success: false,
        message: 'Job de diagnóstico não encontrado',
      })
    }

    return response.ok({
      success: true,
      data: job,
    })
  }
}
