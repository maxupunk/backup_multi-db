import type { ChildProcess } from 'node:child_process'

export type ChildProcessExit = {
  exitCode: number | null
  error?: Error
}

/**
 * Aguarda o encerramento de um processo filho.
 *
 * Consultar `process.exitCode` ao fim da escrita de um stream e' uma corrida: o
 * stream pode terminar antes do evento de saida chegar, e nesse instante o campo
 * ainda e' `null`. Esperar explicitamente por `close` (ou por `error`, quando o
 * binario nem chega a executar) elimina a ambiguidade.
 *
 * `close` e' preferido a `exit` porque so dispara depois que os stdio do
 * processo foram fechados — garantindo que stdout/stderr ja foram capturados.
 */
export function waitForChildProcessExit(child: ChildProcess): Promise<ChildProcessExit> {
  return new Promise((resolve) => {
    child.once('error', (error) => resolve({ exitCode: null, error }))
    child.once('close', (exitCode) => resolve({ exitCode }))
  })
}
