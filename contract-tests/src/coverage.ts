/**
 * Rastro de cobertura de rotas.
 *
 * Cada chamada do cliente HTTP anota qual template do baseline foi
 * exercitado. O rastro vai para um JSONL em modo append porque os testes
 * rodam num processo worker do vitest e o relatorio roda no processo
 * principal — um `Set` em memoria nao atravessaria essa fronteira.
 *
 * Append de linhas curtas e' atomico o bastante para esse uso: cada `write`
 * carrega uma linha inteira e a suite roda serializada.
 */

import { appendFileSync, mkdirSync, readFileSync } from 'node:fs'
import { dirname } from 'node:path'
import { coverageFilePath, loadConfig } from './config.ts'

export interface CoverageEntry {
  /** `METHOD /api/pattern` do baseline, ou `null` se a URL nao casou. */
  key: string | null
  method: string
  path: string
  status: number
  /** Nome do arquivo de teste que fez a chamada, quando disponivel. */
  test?: string
}

let ready = false

function ensureFile(file: string): void {
  if (ready) return
  mkdirSync(dirname(file), { recursive: true })
  ready = true
}

export function recordCoverage(entry: CoverageEntry): void {
  const config = loadConfig()
  const file = coverageFilePath(config)

  try {
    ensureFile(file)
    appendFileSync(file, JSON.stringify(entry) + '\n', 'utf8')
  } catch {
    // Cobertura e' instrumentacao, nao assercao: se o disco falhar, o teste
    // ainda tem que reportar o resultado dele. O relatorio final acusa a
    // ausencia.
  }
}

export function readCoverage(file = coverageFilePath()): CoverageEntry[] {
  let raw: string
  try {
    raw = readFileSync(file, 'utf8')
  } catch {
    return []
  }

  return raw
    .split('\n')
    .filter((line) => line.trim() !== '')
    .map((line) => JSON.parse(line) as CoverageEntry)
}
