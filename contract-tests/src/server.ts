/**
 * Ciclo de vida do servidor sob teste.
 *
 * Cada execucao ganha um diretorio proprio com um SQLite descartavel, entao
 * duas execucoes nunca se enxergam.
 */

import { spawn, spawnSync, type ChildProcess } from 'node:child_process'
import { createWriteStream, existsSync, mkdirSync, rmSync, statSync } from 'node:fs'
import { createServer } from 'node:net'
import { join } from 'node:path'
import { request as undiciRequest } from 'undici'
import { REPO_ROOT, loadConfig, type ContractConfig } from './config.ts'

/**
 * Credenciais fixas da execucao.
 *
 * Sao constantes de teste, nao segredo: o banco e' descartavel e o servidor
 * escuta so' em 127.0.0.1. Fixa-las mantem os golden reproduziveis e permite
 * que o teste de bootstrap saiba qual token mandar.
 */
export const TEST_DB_ENCRYPTION_KEY =
  '000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f'
export const TEST_BOOTSTRAP_TOKEN = 'contract-tests-bootstrap-token'

export interface RunningServer {
  baseUrl: string
  port: number
  /** `null` quando a suite aponta para um servidor externo. */
  stop: () => Promise<void>
}

function findFreePort(preferred: number): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer()
    probe.once('error', reject)
    probe.listen(preferred, '127.0.0.1', () => {
      const address = probe.address()
      const port = typeof address === 'object' && address ? address.port : preferred
      probe.close(() => resolve(port))
    })
  })
}

function prepareWorkDir(config: ContractConfig): void {
  // Recria do zero: sobra de uma execucao anterior e' exatamente o estado
  // nao-determinista que esta suite existe para evitar.
  rmSync(config.workDir, { recursive: true, force: true })
  mkdirSync(config.workDir, { recursive: true })
  mkdirSync(join(config.workDir, 'backups'), { recursive: true })
  mkdirSync(join(config.workDir, 'diagnostics'), { recursive: true })
}

function sqlitePath(config: ContractConfig): string {
  return join(config.workDir, 'app.sqlite3')
}

/** Confirma que o backend criou o banco *dentro* do diretorio da execucao. */
function assertDisposableDatabase(config: ContractConfig): void {
  const file = sqlitePath(config)

  if (!existsSync(file) || statSync(file).size === 0) {
    throw new Error(
      `As migrations rodaram mas ${file} nao existe (ou esta' vazio).\n` +
        `Isso significa que o backend usou OUTRO banco — provavelmente o do ` +
        `\`.env\` da raiz, que e' o de producao. Abortando antes de escrever ` +
        `qualquer coisa nele.`
    )
  }
}

interface SpawnResult {
  child: ChildProcess
  tail: () => string
}

function spawnLogged(
  command: string,
  args: string[],
  cwd: string,
  env: NodeJS.ProcessEnv,
  logFile: string
): SpawnResult {
  const child = spawn(command, args, {
    cwd,
    env,
    detached: process.platform !== 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  })

  const stream = createWriteStream(logFile, { flags: 'a' })
  const buffer: string[] = []

  const capture = (chunk: Buffer) => {
    const text = chunk.toString('utf8')
    stream.write(text)
    buffer.push(text)
    if (buffer.length > 200) buffer.splice(0, buffer.length - 200)
  }

  child.stdout?.on('data', capture)
  child.stderr?.on('data', capture)

  return { child, tail: () => buffer.join('') }
}

function killTree(child: ChildProcess): void {
  if (child.pid === undefined || child.exitCode !== null) return

  if (process.platform === 'win32') {
    spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
    return
  }

  try {
    process.kill(-child.pid, 'SIGTERM')
  } catch {
    try {
      child.kill('SIGKILL')
    } catch {
      // Ja' morreu.
    }
  }
}

function runOnce(
  command: string,
  args: string[],
  cwd: string,
  env: NodeJS.ProcessEnv,
  logFile: string
): Promise<void> {
  return new Promise((resolve, reject) => {
    const { child, tail } = spawnLogged(command, args, cwd, env, logFile)

    child.on('error', reject)
    child.on('exit', (code) => {
      if (code === 0) return resolve()
      reject(
        new Error(`\`${command} ${args.join(' ')}\` saiu com codigo ${code}.\n${tail().slice(-4000)}`)
      )
    })
  })
}

async function waitForHealth(baseUrl: string, timeoutMs: number, tail: () => string): Promise<void> {
  const deadline = Date.now() + timeoutMs
  let lastError: unknown

  while (Date.now() < deadline) {
    try {
      // undici direto, sem passar pelo cliente da suite: a espera de boot faz
      // dezenas de chamadas e nao pode entrar no rastro de cobertura.
      const response = await undiciRequest(`${baseUrl}/api/health`, {
        method: 'GET',
        headersTimeout: 2_000,
        bodyTimeout: 2_000,
      })
      await response.body.dump()
      if (response.statusCode === 200) return
      lastError = new Error(`/api/health respondeu ${response.statusCode}`)
    } catch (error) {
      lastError = error
    }

    await new Promise((resolve) => setTimeout(resolve, 250))
  }

  throw new Error(
    `O servidor nao respondeu 200 em ${baseUrl}/api/health dentro de ${timeoutMs}ms.\n` +
      `Ultimo erro: ${String(lastError)}\n\nLog do servidor:\n${tail().slice(-4000)}`
  )
}

async function startRoco(config: ContractConfig): Promise<RunningServer> {
  const cwd = join(REPO_ROOT, 'back-roco')
  const port = await findFreePort(config.port)
  const logFile = join(config.workDir, 'server.log')

  const env: NodeJS.ProcessEnv = {
    ...process.env,
    LOCO_ENV: 'test',
    PORT: String(port),
    BINDING: '127.0.0.1',
    LOG_LEVEL: config.logLevel,
    DB_ENCRYPTION_KEY: TEST_DB_ENCRYPTION_KEY,
    INITIAL_ADMIN_BOOTSTRAP_TOKEN: TEST_BOOTSTRAP_TOKEN,
    AUTH_ACCESS_TOKEN_EXPIRES_IN: '7d',
    DATABASE_URL: `sqlite://${join(config.workDir, 'app.sqlite3').replace(/\\/g, '/')}?mode=rwc`,
    BACKUP_STORAGE_PATH: join(config.workDir, 'backups'),
    DIAGNOSTICS_PATH: join(config.workDir, 'diagnostics'),
    AUDIT_RETENTION_DAYS: '0',
  }

  // Compila antes de medir o boot: `cargo run` a frio leva minutos e estouraria
  // qualquer timeout razoavel de health check.
  await runOnce('cargo', ['build', '--bin', 'back_roco-cli'], cwd, env, logFile)
  await runOnce('cargo', ['run', '--bin', 'back_roco-cli', '--', 'db', 'migrate'], cwd, env, logFile)
  assertDisposableDatabase(config)

  const { child, tail } = spawnLogged(
    'cargo',
    ['run', '--bin', 'back_roco-cli', '--', 'start'],
    cwd,
    env,
    logFile
  )

  const baseUrl = `http://127.0.0.1:${port}`

  try {
    await waitForHealth(baseUrl, config.bootTimeoutMs, tail)
  } catch (error) {
    killTree(child)
    throw error
  }

  return {
    baseUrl,
    port,
    stop: async () => {
      killTree(child)
      await new Promise((resolve) => setTimeout(resolve, 200))
    },
  }
}

export async function startServer(config = loadConfig()): Promise<RunningServer> {
  if (!config.manageServer) {
    // Servidor externo: quem subiu e' quem derruba, e o estado e' de quem
    // subiu. Util no CI e para depurar contra um servidor ja' rodando.
    return { baseUrl: config.baseUrl, port: 0, stop: async () => {} }
  }

  prepareWorkDir(config)

  return startRoco(config)
}
