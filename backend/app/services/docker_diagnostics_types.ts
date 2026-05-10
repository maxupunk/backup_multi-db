export const diagnosticToolValues = ['ping', 'port_scan', 'curl'] as const

export type DockerDiagnosticTool = (typeof diagnosticToolValues)[number]

export const diagnosticStatusValues = ['pending', 'running', 'completed', 'failed'] as const

export type DockerDiagnosticStatus = (typeof diagnosticStatusValues)[number]

export interface DockerDiagnosticStartPayload {
  tool: DockerDiagnosticTool
  target: string
  port?: number
  count?: number
  timeoutMs?: number
}

export interface DockerDiagnosticJob {
  id: string
  tool: DockerDiagnosticTool
  status: DockerDiagnosticStatus
  target: string
  port: number | null
  count: number | null
  timeoutMs: number | null
  startedAt: string
  completedAt: string | null
  outputLines: string[]
  summary: string | null
  error: string | null
  portOpen: boolean | null
  latencyMs: number | null
}

export interface DockerDiagnosticRunner {
  readonly tool: DockerDiagnosticTool
  run(job: DockerDiagnosticJob, onUpdate: (job: DockerDiagnosticJob) => void): Promise<void>
}
