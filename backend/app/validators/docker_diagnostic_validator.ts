import vine from '@vinejs/vine'
import { diagnosticToolValues } from '#services/docker_diagnostics_types'

export const startDockerDiagnosticValidator = vine.compile(
  vine.object({
    tool: vine.enum(diagnosticToolValues),
    target: vine.string().trim().minLength(1).maxLength(2048),
    port: vine.number().positive().max(65535).optional(),
    count: vine.number().positive().max(10).optional(),
    timeoutMs: vine.number().positive().max(10000).optional(),
  })
)
