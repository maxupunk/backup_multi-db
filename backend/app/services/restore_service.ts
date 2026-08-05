import { spawn } from 'node:child_process'
import { createReadStream, existsSync } from 'node:fs'
import { createGunzip } from 'node:zlib'
import { Transform } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import { StringDecoder } from 'node:string_decoder'
import type { Readable } from 'node:stream'
import logger from '@adonisjs/core/services/logger'
import type Backup from '#models/backup'
import type Connection from '#models/connection'
import { StorageDestinationService } from '#services/storage_destination_service'
import { BackupService } from '#services/backup_service'
import { waitForChildProcessExit } from '#services/child_process_exit'
import { ProcessOutputBuffer } from '#services/process_output_buffer'
import type { RestoreProgressEmitter } from '#services/restore_progress_emitter'

/**
 * Modo de restauração
 */
export type RestoreMode = 'full' | 'schema-only' | 'data-only'

/**
 * Teto para uma única linha do dump mantida em memória.
 *
 * Os filtros trabalham por linha, então uma linha sem quebra faria o buffer
 * crescer sem limite — um dump inteiro numa linha só levaria o processo a
 * estourar o heap. O valor é folgado de propósito: dumps reais (inclusive
 * `mysqldump --extended-insert` com blobs) ficam muito abaixo disso, e a
 * mensagem de erro indica a saída (restaurar sem filtros, que não processa
 * por linha).
 */
const MAX_BUFFERED_LINE_CHARS = 64 * 1024 * 1024

/**
 * Decisões de filtragem aplicadas a cada linha do dump.
 */
type LineFilterRules = {
  /** Linha completa (terminada por quebra de linha). Pode manter estado. */
  keepLine: (line: string) => boolean
  /** Resto final sem quebra de linha — tratado à parte por compatibilidade. */
  keepTrailing: (line: string) => boolean
}

/**
 * Opções de restauração
 */
export interface RestoreOptions {
  /** Modo de restauração: full, schema-only ou data-only */
  mode: RestoreMode
  /** Database de destino (sobrescreve o original) */
  targetDatabase?: string
  /** PostgreSQL: Não restaurar owners (ALTER ... OWNER TO) */
  noOwner?: boolean
  /** PostgreSQL: Não restaurar privilégios (GRANT/REVOKE) */
  noPrivileges?: boolean
  /** PostgreSQL: Não restaurar tablespaces */
  noTablespaces?: boolean
  /** PostgreSQL: Não restaurar comentários (COMMENT ON) */
  noComments?: boolean
  /** MySQL/MariaDB: Não executar CREATE DATABASE / USE */
  noCreateDb?: boolean
  /** Pular verificação e backup de segurança antes da restauração */
  skipSafetyBackup?: boolean
  /** Limpar o banco de destino antes de restaurar (DROP + recriar schema/database) */
  clearBeforeRestore?: boolean
}

/**
 * Informações do backup de segurança criado antes da restauração
 */
export interface SafetyBackupInfo {
  id: number
  fileName: string | null
  fileSize: number | null
  success: boolean
}

/**
 * Resultado de uma restauração
 */
export interface RestoreResult {
  success: boolean
  databaseName: string
  durationSeconds: number
  error?: string
  exitCode?: number
  warnings?: string[]
  /** Backup de segurança criado antes da restauração, se o database existia */
  safetyBackup?: SafetyBackupInfo
}

/**
 * Configuração do comando de restore
 */
interface RestoreConfig {
  command: string
  args: string[]
  env: NodeJS.ProcessEnv
}

/**
 * Serviço responsável por restaurar backups de banco de dados.
 * Suporta MySQL, MariaDB e PostgreSQL.
 */
export class RestoreService {
  private static readonly PROCESS_OUTPUT_CAPTURE_LIMIT_BYTES = 256 * 1024

  /**
   * Restaura um backup para o banco de dados.
   * Se o database de destino existir (e skipSafetyBackup não estiver ativo),
   * cria um backup de segurança antes de restaurar.
   */
  async restore(
    backup: Backup,
    connection: Connection,
    options: RestoreOptions,
    emitter?: RestoreProgressEmitter
  ): Promise<RestoreResult> {
    const startTime = Date.now()
    const targetDb = options.targetDatabase || backup.databaseName

    logger.info(
      `[Restore] Iniciando restauração do backup ID: ${backup.id} ` +
        `para database "${targetDb}" da conexão "${connection.name}" (ID: ${connection.id})`
    )

    let safetyBackup: SafetyBackupInfo | undefined

    try {
      // 1. Validar
      emitter?.validating()

      // 2. Verificar se o database existe e criar backup de segurança
      if (!options.skipSafetyBackup) {
        const dbExists = await this.checkDatabaseExists(connection, targetDb)

        if (dbExists) {
          logger.info(
            `[Restore] Database "${targetDb}" existe. Criando backup de segurança antes da restauração...`
          )

          emitter?.safetyBackupStarted()

          const backupService = new BackupService()
          const safetyResult = await backupService.performSafetyBackup(connection, targetDb)

          safetyBackup = {
            id: safetyResult.backup.id,
            fileName: safetyResult.backup.fileName,
            fileSize: safetyResult.backup.fileSize,
            success: safetyResult.success,
          }

          if (!safetyResult.success) {
            const durationSeconds = Math.round((Date.now() - startTime) / 1000)
            logger.error(
              `[Restore] Backup de segurança falhou para "${targetDb}". Restauração abortada.`
            )
            emitter?.safetyBackupFailed()
            return {
              success: false,
              databaseName: targetDb,
              durationSeconds,
              error: 'Backup de segurança falhou. A restauração foi abortada por segurança.',
              safetyBackup,
            }
          }

          emitter?.safetyBackupCompleted()

          logger.info(
            `[Restore] Backup de segurança criado com sucesso: ID ${safetyResult.backup.id}, ` +
              `arquivo: ${safetyResult.backup.fileName}`
          )
        } else {
          logger.info(
            `[Restore] Database "${targetDb}" não existe. Restauração prosseguirá sem backup de segurança.`
          )
        }
      }

      // 3. Limpar banco de destino, se solicitado
      if (options.clearBeforeRestore) {
        logger.info(`[Restore] Limpando banco de dados "${targetDb}" antes da restauração...`)
        emitter?.clearingDatabase()
        await this.clearDatabase(connection, targetDb)
        logger.info(`[Restore] Banco de dados "${targetDb}" limpo com sucesso.`)
      }

      // 4. Preparar stream
      emitter?.preparing()

      // 4. Obter o stream do arquivo de backup
      const backupStream = await this.getBackupStream(backup)

      // 5. Montar a cadeia de transformação — origem, progresso, descompressão
      // e filtros. A cadeia inteira é entregue ao `pipeline()` em executeRestore,
      // que garante destruição em cascata caso qualquer estágio falhe.
      const totalBytes = backup.fileSize ?? 0
      const stages: Array<Readable | Transform> = [backupStream]

      // Contagem de bytes ANTES da descompressão (fileSize é o tamanho comprimido).
      if (emitter && totalBytes > 0) {
        stages.push(this.createProgressTrackingStream(totalBytes, emitter))
      }

      if (backup.compressed) {
        stages.push(createGunzip())
      }

      stages.push(...this.buildFilterStages(connection.type, options))

      // 6. Construir comando de restore
      const restoreConfig = this.buildRestoreConfig(connection, targetDb)

      // 7. Executar restore
      const result = await this.executeRestore(restoreConfig, stages, targetDb)

      const durationSeconds = Math.round((Date.now() - startTime) / 1000)

      if (result.success) {
        logger.info(
          `[Restore] Restauração concluída com sucesso - Backup ID: ${backup.id}, ` +
            `Database: "${targetDb}", Duração: ${durationSeconds}s`
        )
        emitter?.completed(durationSeconds)
      } else {
        logger.error(
          `[Restore] Falha na restauração - Backup ID: ${backup.id}, ` +
            `Database: "${targetDb}", Erro: ${result.error}`
        )
        emitter?.failed(result.error ?? 'Falha na restauração')
      }

      return { ...result, durationSeconds, safetyBackup }
    } catch (error) {
      const durationSeconds = Math.round((Date.now() - startTime) / 1000)
      const errorMessage = error instanceof Error ? error.message : 'Erro desconhecido'
      logger.error(
        `[Restore] Erro inesperado na restauração - Backup ID: ${backup.id}, Erro: ${errorMessage}`
      )
      emitter?.failed(errorMessage)
      return {
        success: false,
        databaseName: targetDb,
        durationSeconds,
        error: errorMessage,
        safetyBackup,
      }
    }
  }

  /**
   * Cria um Transform stream que conta bytes e emite progresso.
   * Posicionado antes do gunzip para contagem precisa (bytes comprimidos = fileSize do backup).
   */
  private createProgressTrackingStream(
    totalBytes: number,
    emitter: RestoreProgressEmitter
  ): Transform {
    let bytesRead = 0

    return new Transform({
      transform(chunk, _encoding, callback) {
        bytesRead += chunk.length
        const percent = (bytesRead / totalBytes) * 100
        emitter.restoring(percent)
        this.push(chunk)
        callback()
      },
    })
  }

  /**
   * Verifica se um database existe na conexão especificada.
   * Usa o comando nativo do banco (psql / mysql) para evitar construção de SQL.
   */
  private checkDatabaseExists(connection: Connection, databaseName: string): Promise<boolean> {
    return new Promise((resolve) => {
      const password = connection.getDecryptedPassword()
      let command: string
      let args: string[]
      let env: NodeJS.ProcessEnv

      if (connection.type === 'postgresql') {
        // Conecta diretamente no database — se não existir, o processo falha
        command = 'psql'
        args = [
          '-h',
          connection.host,
          '-p',
          connection.port.toString(),
          '-U',
          connection.username,
          '-d',
          databaseName,
          '--no-password',
          '--tuples-only',
          '--quiet',
          '-c',
          'SELECT 1',
        ]
        env = { ...process.env, PGPASSWORD: password }
      } else {
        // MySQL / MariaDB: tenta usar o database
        command = 'mysql'
        args = [
          '-h',
          connection.host,
          '-P',
          connection.port.toString(),
          '-u',
          connection.username,
          `--password=${password}`,
          ...connection.getMysqlSslArgs(),
          '-e',
          'SELECT 1',
          databaseName,
        ]
        env = { ...process.env }
      }

      const proc = spawn(command, args, { env, stdio: ['ignore', 'pipe', 'pipe'] })

      proc.on('error', () => resolve(false))
      proc.on('close', (code) => resolve(code === 0))
    })
  }

  /**
   * Limpa todos os objetos do banco de destino antes da restauração.
   * PostgreSQL: descarta e recria o schema public.
   * MySQL/MariaDB: dropa e recria o database.
   */
  private clearDatabase(connection: Connection, databaseName: string): Promise<void> {
    const password = connection.getDecryptedPassword()
    let command: string
    let args: string[]
    let env: NodeJS.ProcessEnv

    if (connection.type === 'postgresql') {
      command = 'psql'
      args = [
        '-h',
        connection.host,
        '-p',
        connection.port.toString(),
        '-U',
        connection.username,
        '-d',
        databaseName,
        '--no-password',
        '-c',
        'DROP SCHEMA public CASCADE;',
        '-c',
        'CREATE SCHEMA public;',
        '-c',
        'GRANT ALL ON SCHEMA public TO public;',
      ]
      env = { ...process.env, PGPASSWORD: password }
    } else {
      // MySQL / MariaDB: escapa backticks no nome do banco para evitar SQL injection
      const safeName = databaseName.replace(/`/g, '``')
      command = 'mysql'
      args = [
        '-h',
        connection.host,
        '-P',
        connection.port.toString(),
        '-u',
        connection.username,
        `--password=${password}`,
        ...connection.getMysqlSslArgs(),
        '-e',
        `DROP DATABASE IF EXISTS \`${safeName}\`; CREATE DATABASE \`${safeName}\`;`,
      ]
      env = { ...process.env }
    }

    return new Promise<void>((resolve, reject) => {
      const proc = spawn(command, args, { env, stdio: ['ignore', 'pipe', 'pipe'] })
      const stderr: string[] = []

      proc.stderr?.on('data', (chunk: Buffer) => stderr.push(chunk.toString()))
      proc.on('error', (err) => reject(err))
      proc.on('close', (code) => {
        if (code === 0) {
          resolve()
        } else {
          reject(
            new Error(
              `Falha ao limpar o banco de dados (exit code ${code}): ${stderr.join('').trim()}`
            )
          )
        }
      })
    })
  }

  /**
   * Obtém o stream do arquivo de backup (local ou remoto)
   */
  private async getBackupStream(backup: Backup): Promise<Readable> {
    if (!backup.filePath) {
      throw new Error('Arquivo de backup não disponível')
    }

    // Tentar carregar storage destination
    await backup.load('storageDestination')
    const destination = backup.storageDestination ?? null
    const fullPath = StorageDestinationService.getLocalFullPath(destination, backup.filePath)

    // Verificar se o arquivo existe localmente
    if (existsSync(fullPath)) {
      return createReadStream(fullPath)
    }

    // Tentar buscar do storage remoto
    if (destination) {
      const download = await StorageDestinationService.getDownloadStream(
        destination,
        backup.filePath
      )
      return download.stream
    }

    throw new Error('Arquivo de backup não encontrado no servidor')
  }

  /**
   * Monta os filtros de transformação aplicáveis às opções escolhidas.
   *
   * Devolve os estágios em vez de encadeá-los com `.pipe()`: quem monta a cadeia
   * usa `pipeline()`, que destrói todos os estágios quando um deles falha.
   * Com `.pipe()` encadeado, uma falha no meio (gunzip de arquivo corrompido,
   * por exemplo) deixava o stream de origem aberto — descritor de arquivo ou
   * socket S3 pendurado até o GC.
   */
  private buildFilterStages(dbType: string, options: RestoreOptions): Transform[] {
    const rules = this.buildLineRules(dbType, options)

    return rules ? [this.createRestoreFilter(rules)] : []
  }

  /**
   * Monta as regras de decisão por linha para as opções escolhidas.
   *
   * Retorna `null` quando nenhuma opção de filtragem está ativa — nesse caso o
   * dump passa direto, sem nenhum Transform no caminho.
   */
  private buildLineRules(dbType: string, options: RestoreOptions): LineFilterRules | null {
    const modeRules = this.buildModeRules(dbType, options.mode)
    const dropPatterns = this.buildDropPatterns(dbType, options)

    if (!modeRules && dropPatterns.length === 0) {
      return null
    }

    const isDropped = (line: string): boolean => dropPatterns.some((pattern) => pattern.test(line))

    return {
      keepLine: (line) => (modeRules ? modeRules.keepLine(line) : true) && !isDropped(line),
      keepTrailing: (line) => (modeRules ? modeRules.keepTrailing(line) : true) && !isDropped(line),
    }
  }

  /**
   * Regras derivadas do modo de restauração (full / schema-only / data-only).
   *
   * `keepTrailing` existe porque o resto sem quebra de linha final era tratado
   * de forma diferente por cada filtro na implementação encadeada: os filtros
   * data-only emitiam esse resto sem reavaliar a allowlist. O comportamento é
   * preservado explicitamente para que a saída continue byte a byte idêntica.
   */
  private buildModeRules(dbType: string, mode: RestoreMode): LineFilterRules | null {
    if (mode === 'schema-only') {
      return dbType === 'postgresql'
        ? this.buildPostgresSchemaOnlyRules()
        : this.buildMysqlSchemaOnlyRules()
    }

    if (mode === 'data-only') {
      return dbType === 'postgresql'
        ? this.buildPostgresDataOnlyRules()
        : this.buildMysqlDataOnlyRules()
    }

    return null
  }

  /**
   * schema-only PostgreSQL: descarta blocos COPY ... \. e instruções INSERT.
   */
  private buildPostgresSchemaOnlyRules(): LineFilterRules {
    let insideCopyBlock = false

    return {
      keepLine: (line) => {
        if (insideCopyBlock) {
          if (line === '\\.') {
            insideCopyBlock = false
          }
          return false
        }

        if (/^COPY\s+.*\s+FROM\s+stdin/i.test(line)) {
          insideCopyBlock = true
          return false
        }

        return !/^\s*INSERT\s+INTO\s+/i.test(line)
      },
      keepTrailing: (line) => !insideCopyBlock && !/^\s*INSERT\s+INTO\s+/i.test(line),
    }
  }

  /**
   * schema-only MySQL/MariaDB: descarta INSERT INTO, LOCK TABLES e UNLOCK TABLES.
   */
  private buildMysqlSchemaOnlyRules(): LineFilterRules {
    return {
      keepLine: (line) =>
        !/^\s*INSERT\s+INTO\s+/i.test(line) &&
        !/^\s*LOCK\s+TABLES\s+/i.test(line) &&
        !/^\s*UNLOCK\s+TABLES/i.test(line),
      keepTrailing: (line) => !/^\s*(INSERT\s+INTO|LOCK\s+TABLES|UNLOCK\s+TABLES)/i.test(line),
    }
  }

  /**
   * data-only PostgreSQL: mantém blocos COPY, INSERTs, configuração de sessão,
   * comentários e o disable/enable de triggers exigido pelo COPY.
   */
  private buildPostgresDataOnlyRules(): LineFilterRules {
    let insideCopyBlock = false

    return {
      keepLine: (line) => {
        if (insideCopyBlock) {
          if (line === '\\.') {
            insideCopyBlock = false
          }
          return true
        }

        if (/^COPY\s+.*\s+FROM\s+stdin/i.test(line)) {
          insideCopyBlock = true
          return true
        }

        if (/^\s*INSERT\s+INTO\s+/i.test(line)) return true
        if (/^\s*(SET|BEGIN|COMMIT|ROLLBACK|SELECT\s+pg_catalog\.set_config)/i.test(line)) {
          return true
        }
        if (/^\s*--/.test(line) || line.trim() === '') return true
        if (/^\s*ALTER\s+TABLE\s+.*\s+(DISABLE|ENABLE)\s+TRIGGER/i.test(line)) return true

        return false
      },
      // Preserva o comportamento anterior: o resto sem quebra final era emitido
      // sem passar pela allowlist.
      keepTrailing: () => true,
    }
  }

  /**
   * data-only MySQL/MariaDB: mantém INSERTs, LOCK/UNLOCK, SETs e comentários.
   */
  private buildMysqlDataOnlyRules(): LineFilterRules {
    return {
      keepLine: (line) => {
        if (/^\s*INSERT\s+INTO\s+/i.test(line)) return true
        if (/^\s*(LOCK\s+TABLES|UNLOCK\s+TABLES)/i.test(line)) return true
        if (/^\s*SET\s+/i.test(line) || /^\s*\/\*!\d+\s+SET\s+/i.test(line)) return true
        if (/^\s*--/.test(line) || /^\s*\/\*/.test(line) || line.trim() === '') return true

        return false
      },
      keepTrailing: () => true,
    }
  }

  /**
   * Padrões de linha descartados conforme as opções de compatibilidade.
   */
  private buildDropPatterns(dbType: string, options: RestoreOptions): RegExp[] {
    const patterns: RegExp[] = []

    if (dbType === 'postgresql') {
      if (options.noOwner) patterns.push(/^\s*ALTER\s+.*\s+OWNER\s+TO\s+/i)
      if (options.noPrivileges) patterns.push(/^\s*(GRANT|REVOKE)\s+/i)
      if (options.noTablespaces) patterns.push(/^\s*SET\s+default_tablespace\s*=/i)
      if (options.noComments) patterns.push(/^\s*COMMENT\s+ON\s+/i)
    }

    if ((dbType === 'mysql' || dbType === 'mariadb') && options.noCreateDb) {
      patterns.push(/^\s*(CREATE\s+DATABASE|USE\s+`)/i)
    }

    return patterns
  }

  /**
   * Transform único que aplica todas as regras numa só passagem.
   *
   * A implementação anterior encadeava até cinco Transforms, cada um com o seu
   * próprio buffer de linha parcial. Uma linha patológica (um INSERT gigante com
   * blob, por exemplo) era mantida em memória uma vez por estágio — cinco cópias
   * do mesmo trecho. Com um estágio só existe uma cópia, limitada pelo teto abaixo.
   *
   * O `StringDecoder` guarda os bytes incompletos entre chunks: `chunk.toString()`
   * quebrava caracteres UTF-8 multibyte que caíssem na fronteira, corrompendo
   * silenciosamente a acentuação no banco restaurado.
   */
  private createRestoreFilter(
    rules: LineFilterRules,
    maxBufferedChars = MAX_BUFFERED_LINE_CHARS
  ): Transform {
    let buffer = ''
    const decoder = new StringDecoder('utf8')

    return new Transform({
      transform(chunk, _encoding, callback) {
        buffer += decoder.write(chunk)

        const lines = buffer.split('\n')
        // A última parte pode ser uma linha incompleta; fica para o próximo chunk.
        buffer = lines.pop() || ''

        if (buffer.length > maxBufferedChars) {
          callback(
            new Error(
              `Linha do dump excede ${Math.round(maxBufferedChars / (1024 * 1024))} MB sem quebra de linha. ` +
                'Restaure em modo "full" sem opções de compatibilidade para evitar o processamento por linha.'
            )
          )
          return
        }

        const output: string[] = []
        for (const line of lines) {
          if (rules.keepLine(line)) {
            output.push(line)
          }
        }

        if (output.length > 0) {
          this.push(output.join('\n') + '\n')
        }

        callback()
      },
      flush(callback) {
        buffer += decoder.end()

        if (buffer && rules.keepTrailing(buffer)) {
          this.push(buffer + '\n')
        }

        callback()
      },
    })
  }

  /**
   * Constrói a configuração do comando de restore
   */
  private buildRestoreConfig(connection: Connection, targetDatabase: string): RestoreConfig {
    const password = connection.getDecryptedPassword()

    if (connection.type === 'postgresql') {
      return {
        command: 'psql',
        args: [
          '-h',
          connection.host,
          '-p',
          connection.port.toString(),
          '-U',
          connection.username,
          '-d',
          targetDatabase,
          '--no-password',
          '-v',
          'ON_ERROR_STOP=1',
        ],
        env: { ...process.env, PGPASSWORD: password },
      }
    }

    // MySQL / MariaDB
    return {
      command: 'mysql',
      args: [
        '-h',
        connection.host,
        '-P',
        connection.port.toString(),
        '-u',
        connection.username,
        `--password=${password}`,
        ...connection.getMysqlSslArgs(),
        targetDatabase,
      ],
      env: process.env,
    }
  }

  /**
   * Executa o processo de restore alimentando seu stdin pela cadeia de streams.
   */
  private async executeRestore(
    config: RestoreConfig,
    stages: Array<Readable | Transform>,
    databaseName: string
  ): Promise<RestoreResult> {
    const restoreProcess = spawn(config.command, config.args, {
      env: config.env,
      stdio: ['pipe', 'pipe', 'pipe'],
    })

    const stderrBuffer = new ProcessOutputBuffer(RestoreService.PROCESS_OUTPUT_CAPTURE_LIMIT_BYTES)
    const stdoutBuffer = new ProcessOutputBuffer(RestoreService.PROCESS_OUTPUT_CAPTURE_LIMIT_BYTES)

    restoreProcess.stdout?.on('data', (data: Buffer) => {
      stdoutBuffer.append(data)
    })

    restoreProcess.stderr?.on('data', (data: Buffer) => {
      stderrBuffer.append(data)
    })

    // Registrado antes do pipeline para não perder o evento de saída.
    const exitPromise = waitForChildProcessExit(restoreProcess)

    let streamError: Error | null = null

    try {
      await pipeline([...stages, restoreProcess.stdin!])
    } catch (error) {
      streamError = error instanceof Error ? error : new Error(String(error))
    }

    const exit = await exitPromise
    const stderrData = stderrBuffer.toString()
    const stdoutData = stdoutBuffer.toString()
    const warnings = this.extractWarnings(stderrData)

    if (exit.error) {
      return {
        success: false,
        databaseName,
        durationSeconds: 0,
        error:
          `Falha ao executar ${config.command}: ${exit.error.message}. ` +
          'Verifique se o binário está instalado e no PATH.',
      }
    }

    // O desfecho do processo tem precedência sobre o erro de stream. Quando o
    // banco aborta no meio (psql com ON_ERROR_STOP=1), a escrita restante no
    // stdin falha — com código EPIPE no Linux, EOF no Windows. Tentar classificar
    // esse erro pelo código seria frágil; o stderr do banco explica a causa real.
    if (exit.exitCode !== 0) {
      if (streamError) {
        logger.debug(
          `[Restore] stdin interrompido (${streamError.message}); reportando o erro do processo`
        )
      }

      return {
        success: false,
        databaseName,
        durationSeconds: 0,
        error: stderrData || stdoutData || `Processo terminou com código ${exit.exitCode}`,
        exitCode: exit.exitCode ?? undefined,
        warnings: warnings.length > 0 ? warnings : undefined,
      }
    }

    // Processo terminou bem, mas a cadeia quebrou: o dump foi entregue pela
    // metade (arquivo corrompido, download interrompido). Reportar sucesso aqui
    // seria mentir sobre o estado do banco restaurado.
    if (streamError) {
      return {
        success: false,
        databaseName,
        durationSeconds: 0,
        error: `Erro ao ler arquivo de backup: ${streamError.message}`,
        exitCode: exit.exitCode ?? undefined,
        warnings: warnings.length > 0 ? warnings : undefined,
      }
    }

    return {
      success: true,
      databaseName,
      durationSeconds: 0,
      exitCode: 0,
      warnings: warnings.length > 0 ? warnings : undefined,
    }
  }

  /**
   * Extrai avisos (warnings) da saída de erro
   */
  private extractWarnings(stderr: string): string[] {
    if (!stderr) return []
    return stderr
      .split('\n')
      .filter((line) => /warning|notice|info/i.test(line))
      .map((line) => line.trim())
      .filter(Boolean)
  }
}
