/**
 * Configuracao da suite de contrato (tarefa 1.2 do roadmap).
 *
 * Tudo vem de variaveis de ambiente porque a suite precisa ser lancavel do
 * mesmo jeito por `scripts/run.mjs`, pelo CI e por um `vitest` cru rodado a
 * mao. Nada aqui le arquivo de configuracao: se um valor nao esta no ambiente,
 * o default abaixo e' o contrato.
 */

import { fileURLToPath } from 'node:url'
import { dirname, join, resolve } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))

/** Raiz de `contract-tests/`. */
export const CONTRACT_ROOT = resolve(here, '..')

/** Raiz do repositorio — onde vivem `backend/`, `back-roco/` e `docs/`. */
export const REPO_ROOT = resolve(CONTRACT_ROOT, '..')

/** Qual implementacao esta sob teste. A suite e' identica para as duas. */
export type Target = 'adonis' | 'roco'

/**
 * O que fazer com os golden files.
 *
 * - `off`: ignora golden, so' roda as assercoes escritas no teste.
 * - `record`: sobrescreve o golden com a resposta observada.
 * - `compare`: falha se a resposta divergir do golden.
 */
export type GoldenMode = 'off' | 'record' | 'compare'

export interface ContractConfig {
  target: Target
  /** URL base ja' sem barra final, ex.: `http://127.0.0.1:3399`. */
  baseUrl: string
  /** Se true, o harness sobe e derruba o servidor sob teste. */
  manageServer: boolean
  /** Porta a usar quando o harness gerencia o servidor. 0 = escolher livre. */
  port: number
  /** Timeout de uma requisicao isolada. */
  requestTimeoutMs: number
  /** Quanto esperar o servidor responder /api/health no boot. */
  bootTimeoutMs: number
  /**
   * Tentativas extras para falha de *conexao* (nunca para status HTTP, e
   * nunca para metodos nao idempotentes — ver `src/http.ts`).
   */
  retries: number
  retryDelayMs: number
  goldenMode: GoldenMode
  goldenDir: string
  /** Diretorio de trabalho da execucao: SQLite, logs, state.json, cobertura. */
  workDir: string
  /** Identificador desta execucao. */
  runId: string
  /** Emite `reports/contract-diff.md` ao final (usado por `contract:diff`). */
  emitDiffReport: boolean
  /** Falha o processo se alguma rota do baseline ficar sem teste (1.8). */
  enforceRouteCoverage: boolean
  logLevel: string
}

function envString(name: string, fallback: string): string {
  const value = process.env[name]
  return value === undefined || value === '' ? fallback : value
}

function envNumber(name: string, fallback: number): number {
  const raw = process.env[name]
  if (raw === undefined || raw === '') return fallback

  const parsed = Number(raw)
  if (!Number.isFinite(parsed)) {
    throw new Error(`${name} precisa ser numerico, recebi ${JSON.stringify(raw)}`)
  }
  return parsed
}

function envBoolean(name: string, fallback: boolean): boolean {
  const raw = process.env[name]
  if (raw === undefined || raw === '') return fallback
  return raw === '1' || raw.toLowerCase() === 'true'
}

function envEnum<T extends string>(name: string, allowed: readonly T[], fallback: T): T {
  const raw = process.env[name]
  if (raw === undefined || raw === '') return fallback

  if (!(allowed as readonly string[]).includes(raw)) {
    throw new Error(`${name} precisa ser um de ${allowed.join(' | ')}, recebi ${JSON.stringify(raw)}`)
  }
  return raw as T
}

let cached: ContractConfig | null = null

export function loadConfig(): ContractConfig {
  if (cached) return cached

  const target = envEnum<Target>('CONTRACT_TARGET', ['adonis', 'roco'], 'adonis')
  const runId = envString('CONTRACT_RUN_ID', 'local')
  const workDir = resolve(envString('CONTRACT_WORK_DIR', join(CONTRACT_ROOT, '.contract', runId)))

  // `CONTRACT_BASE_URL` explicito implica servidor externo: nao faz sentido o
  // harness subir um servidor e ignorar a URL que mandaram usar.
  const explicitBaseUrl = process.env.CONTRACT_BASE_URL
  const manageServer = envBoolean('CONTRACT_MANAGE_SERVER', explicitBaseUrl === undefined)

  cached = {
    target,
    baseUrl: (explicitBaseUrl ?? 'http://127.0.0.1:3399').replace(/\/+$/, ''),
    manageServer,
    port: envNumber('CONTRACT_PORT', 0),
    requestTimeoutMs: envNumber('CONTRACT_REQUEST_TIMEOUT_MS', 30_000),
    bootTimeoutMs: envNumber('CONTRACT_BOOT_TIMEOUT_MS', 120_000),
    retries: envNumber('CONTRACT_RETRIES', 2),
    retryDelayMs: envNumber('CONTRACT_RETRY_DELAY_MS', 250),
    goldenMode: envEnum<GoldenMode>('CONTRACT_GOLDEN', ['off', 'record', 'compare'], 'compare'),
    goldenDir: resolve(envString('CONTRACT_GOLDEN_DIR', join(CONTRACT_ROOT, '__golden__'))),
    workDir,
    runId,
    emitDiffReport: envBoolean('CONTRACT_DIFF_REPORT', false),
    enforceRouteCoverage: envBoolean('CONTRACT_ENFORCE_COVERAGE', false),
    logLevel: envString('CONTRACT_LOG_LEVEL', 'warn'),
  }

  return cached
}

/**
 * Reaponta a suite para a URL do servidor recem-subido.
 *
 * A porta so' e' conhecida depois do boot (o default e' porta livre
 * automatica), entao a configuracao precisa ser corrigida em memoria. Tambem
 * grava em `process.env` porque os workers do vitest sao processos separados,
 * forkados depois do globalSetup, e herdam o ambiente.
 */
export function setBaseUrl(baseUrl: string): void {
  const config = loadConfig()
  config.baseUrl = baseUrl.replace(/\/+$/, '')
  process.env.CONTRACT_BASE_URL = config.baseUrl
}

/** Caminho do `state.json` — canal entre o globalSetup e os workers do vitest. */
export function stateFilePath(config = loadConfig()): string {
  return join(config.workDir, 'state.json')
}

/** Rastro de cobertura de rotas, em JSONL (append-only, seguro entre processos). */
export function coverageFilePath(config = loadConfig()): string {
  return join(config.workDir, 'route-coverage.jsonl')
}

/** Divergencias encontradas no modo `compare`, em JSONL. */
export function diffFilePath(config = loadConfig()): string {
  return join(config.workDir, 'golden-diff.jsonl')
}
