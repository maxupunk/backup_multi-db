/**
 * Ciclo de vida do servidor sob teste (parte da tarefa 1.4 do roadmap).
 *
 * ## Por que o harness sobe o proprio servidor
 *
 * A alternativa registrada no roadmap era um `POST /api/__test__/reset`
 * habilitado so' em ambiente de teste. Foi descartada por dois motivos:
 *
 * - a decisao D8 (big-bang) exige o `backend/` congelado; abrir rota nova nele
 *   so' para testar contraria isso;
 * - o rate limiter do Adonis usa store **em memoria** (`config/limiter.ts`).
 *   Um endpoint de reset limparia o banco mas nao os contadores do limiter,
 *   e o limiter de `auth` e' de 5 req/min. Reiniciar o processo zera as duas
 *   coisas de uma vez.
 *
 * Cada execucao ganha um diretorio proprio com um SQLite descartavel, entao
 * duas execucoes nunca se enxergam.
 *
 * ## Salvaguarda
 *
 * O backend le' o `.env` da raiz do repositorio, onde vive o caminho do banco
 * de **producao**. As variaveis passadas aqui tem precedencia sobre o `.env`
 * (verificado em `@adonisjs/env`: `process.env` vence), mas depender disso em
 * silencio seria imprudente — `assertDisposableDatabase` confirma, apos as
 * migrations, que o arquivo nasceu dentro do diretorio da execucao. Se nao
 * nasceu, a suite aborta antes de escrever qualquer coisa.
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
export const TEST_APP_KEY = 'contract_tests_app_key_32_chars!!'
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

function adonisEnv(config: ContractConfig, port: number): NodeJS.ProcessEnv {
  return {
    ...process.env,
    NODE_ENV: 'development',
    HOST: '127.0.0.1',
    PORT: String(port),
    LOG_LEVEL: config.logLevel,
    APP_KEY: TEST_APP_KEY,
    DB_ENCRYPTION_KEY: TEST_DB_ENCRYPTION_KEY,
    INITIAL_ADMIN_BOOTSTRAP_TOKEN: TEST_BOOTSTRAP_TOKEN,
    AUTH_ACCESS_TOKEN_EXPIRES_IN: '7d',
    // Tudo que escreve em disco apontado para o diretorio da execucao.
    SQLITE_DATABASE_PATH: sqlitePath(config),
    BACKUP_STORAGE_PATH: join(config.workDir, 'backups'),
    DIAGNOSTICS_PATH: join(config.workDir, 'diagnostics'),
    // Poda de auditoria desligada: apagaria registros no meio da suite.
    AUDIT_RETENTION_DAYS: '0',
  }
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
    // No POSIX, `detached` poe o filho num grupo proprio para que o `serve` do
    // Adonis — que por sua vez abre um processo filho — morra junto.
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
    // `taskkill /T` derruba a arvore inteira. Sem isso o processo interno do
    // `ace serve` sobrevive e continua segurando a porta.
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

async function startAdonis(config: ContractConfig): Promise<RunningServer> {
  const cwd = join(REPO_ROOT, 'backend')
  const port = await findFreePort(config.port)
  const env = adonisEnv(config, port)
  const logFile = join(config.workDir, 'server.log')

  // `node ace migration:run` antes de subir: o Adonis nao migra no boot, e um
  // banco recem-criado nao tem tabela nenhuma.
  await runOnce(process.execPath, ['ace', 'migration:run', '--force'], cwd, env, logFile)
  assertDisposableDatabase(config)

  // `--no-hmr`: o watcher do assembler reinicia o processo a cada toque em
  // arquivo e reiniciar no meio da suite zeraria o rate limiter em memoria,
  // tornando os testes de 429 nao-deterministas.
  const { child, tail } = spawnLogged(
    process.execPath,
    ['ace', 'serve', '--no-hmr'],
    cwd,
    env,
    logFile
  )

  const baseUrl = `http://127.0.0.1:${port}`

  let exitedEarly: string | null = null
  child.on('exit', (code) => {
    exitedEarly = `o processo do servidor saiu com codigo ${code} antes de ficar pronto`
  })

  try {
    await waitForHealth(baseUrl, config.bootTimeoutMs, tail)
  } catch (error) {
    killTree(child)
    if (exitedEarly) throw new Error(`${exitedEarly}\n\n${(error as Error).message}`)
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

async function startRoco(config: ContractConfig): Promise<RunningServer> {
  const cwd = join(REPO_ROOT, 'back-roco')
  const port = await findFreePort(config.port)
  const logFile = join(config.workDir, 'server.log')

  const env: NodeJS.ProcessEnv = {
    ...process.env,
    LOCO_ENV: 'test',
    PORT: String(port),
    BINDING: '127.0.0.1',
    DB_ENCRYPTION_KEY: TEST_DB_ENCRYPTION_KEY,
    INITIAL_ADMIN_BOOTSTRAP_TOKEN: TEST_BOOTSTRAP_TOKEN,
    DATABASE_URL: `sqlite://${join(config.workDir, 'app.sqlite3').replace(/\\/g, '/')}?mode=rwc`,
  }

  // Compila antes de medir o boot: `cargo run` a frio leva minutos e estouraria
  // qualquer timeout razoavel de health check.
  await runOnce('cargo', ['build', '--bin', 'back_roco-cli'], cwd, env, logFile)
  await runOnce('cargo', ['run', '--bin', 'back_roco-cli', '--', 'db', 'migrate'], cwd, env, logFile)

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

  return config.target === 'adonis' ? startAdonis(config) : startRoco(config)
}
