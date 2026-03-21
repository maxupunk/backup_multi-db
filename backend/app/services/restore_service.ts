import { spawn } from 'node:child_process'
import { createReadStream, existsSync } from 'node:fs'
import { createGunzip } from 'node:zlib'
import { Transform } from 'node:stream'
import type { Readable } from 'node:stream'
import logger from '@adonisjs/core/services/logger'
import Backup from '#models/backup'
import Connection from '#models/connection'
import { StorageDestinationService } from '#services/storage_destination_service'
import { BackupService } from '#services/backup_service'
import type { RestoreProgressEmitter } from '#services/restore_progress_emitter'

/**
 * Modo de restauração
 */
export type RestoreMode = 'full' | 'schema-only' | 'data-only'

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
          logger.info(`[Restore] Database "${targetDb}" não existe. Restauração prosseguirá sem backup de segurança.`)
        }
      }

      // 3. Preparar stream
      emitter?.preparing()

      // 4. Obter o stream do arquivo de backup
      const backupStream = await this.getBackupStream(backup)

      // 5. Inserir contagem de bytes para progresso (antes da descompressão)
      const totalBytes = backup.fileSize ?? 0
      const trackedStream = emitter && totalBytes > 0
        ? backupStream.pipe(this.createProgressTrackingStream(totalBytes, emitter))
        : backupStream

      // 6. Descomprimir se necessário
      const sqlStream = backup.compressed ? trackedStream.pipe(createGunzip()) : trackedStream

      // 7. Aplicar filtros conforme as opções
      const filteredStream = this.applyFilters(sqlStream, connection.type, options)

      // 8. Construir comando de restore
      const restoreConfig = this.buildRestoreConfig(connection, targetDb)

      // 9. Executar restore
      const result = await this.executeRestore(restoreConfig, filteredStream, targetDb)

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
          '-h', connection.host,
          '-p', connection.port.toString(),
          '-U', connection.username,
          '-d', databaseName,
          '--no-password',
          '--tuples-only',
          '--quiet',
          '-c', 'SELECT 1',
        ]
        env = { ...process.env, PGPASSWORD: password }
      } else {
        // MySQL / MariaDB: tenta usar o database
        command = 'mysql'
        args = [
          '-h', connection.host,
          '-P', connection.port.toString(),
          '-u', connection.username,
          `--password=${password}`,
          '-e', 'SELECT 1',
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
      const download = await StorageDestinationService.getDownloadStream(destination, backup.filePath)
      return download.stream
    }

    throw new Error('Arquivo de backup não encontrado no servidor')
  }

  /**
   * Aplica filtros de transformação no stream SQL baseado nas opções
   */
  private applyFilters(
    stream: Readable,
    dbType: string,
    options: RestoreOptions
  ): Readable {
    let currentStream: Readable = stream

    if (options.mode === 'schema-only') {
      currentStream = currentStream.pipe(this.createSchemaOnlyFilter(dbType))
    } else if (options.mode === 'data-only') {
      currentStream = currentStream.pipe(this.createDataOnlyFilter(dbType))
    }

    if (dbType === 'postgresql') {
      if (options.noOwner) {
        currentStream = currentStream.pipe(this.createLineFilter(/^\s*ALTER\s+.*\s+OWNER\s+TO\s+/i))
      }
      if (options.noPrivileges) {
        currentStream = currentStream.pipe(
          this.createLineFilter(/^\s*(GRANT|REVOKE)\s+/i)
        )
      }
      if (options.noTablespaces) {
        currentStream = currentStream.pipe(
          this.createLineFilter(/^\s*SET\s+default_tablespace\s*=/i)
        )
      }
      if (options.noComments) {
        currentStream = currentStream.pipe(this.createLineFilter(/^\s*COMMENT\s+ON\s+/i))
      }
    }

    if ((dbType === 'mysql' || dbType === 'mariadb') && options.noCreateDb) {
      currentStream = currentStream.pipe(
        this.createLineFilter(/^\s*(CREATE\s+DATABASE|USE\s+`)/i)
      )
    }

    return currentStream
  }

  /**
   * Cria filtro para modo "schema-only" (remove INSERT/COPY/dados)
   */
  private createSchemaOnlyFilter(dbType: string): Transform {
    if (dbType === 'postgresql') {
      return this.createPostgresSchemaOnlyFilter()
    }
    return this.createMysqlSchemaOnlyFilter()
  }

  /**
   * Filtro schema-only para PostgreSQL
   * Remove blocos COPY ... \. e instruções INSERT
   */
  private createPostgresSchemaOnlyFilter(): Transform {
    let insideCopyBlock = false
    let buffer = ''

    return new Transform({
      transform(chunk, _encoding, callback) {
        buffer += chunk.toString()
        const lines = buffer.split('\n')
        // Manter a última parte como buffer (pode estar incompleta)
        buffer = lines.pop() || ''

        const output: string[] = []
        for (const line of lines) {
          if (insideCopyBlock) {
            // Fim do bloco COPY (linha com apenas \.)
            if (line === '\\.') {
              insideCopyBlock = false
            }
            continue
          }

          // Início de um bloco COPY ... FROM stdin
          if (/^COPY\s+.*\s+FROM\s+stdin/i.test(line)) {
            insideCopyBlock = true
            continue
          }

          // Remover INSERTs
          if (/^\s*INSERT\s+INTO\s+/i.test(line)) {
            continue
          }

          output.push(line)
        }

        if (output.length > 0) {
          this.push(output.join('\n') + '\n')
        }
        callback()
      },
      flush(callback) {
        if (buffer && !insideCopyBlock && !/^\s*INSERT\s+INTO\s+/i.test(buffer)) {
          this.push(buffer + '\n')
        }
        callback()
      },
    })
  }

  /**
   * Filtro schema-only para MySQL/MariaDB
   * Remove INSERT INTO, LOCK TABLES, UNLOCK TABLES
   */
  private createMysqlSchemaOnlyFilter(): Transform {
    let buffer = ''

    return new Transform({
      transform(chunk, _encoding, callback) {
        buffer += chunk.toString()
        const lines = buffer.split('\n')
        buffer = lines.pop() || ''

        const output: string[] = []
        for (const line of lines) {
          if (/^\s*INSERT\s+INTO\s+/i.test(line)) continue
          if (/^\s*LOCK\s+TABLES\s+/i.test(line)) continue
          if (/^\s*UNLOCK\s+TABLES/i.test(line)) continue
          output.push(line)
        }

        if (output.length > 0) {
          this.push(output.join('\n') + '\n')
        }
        callback()
      },
      flush(callback) {
        if (buffer && !/^\s*(INSERT\s+INTO|LOCK\s+TABLES|UNLOCK\s+TABLES)/i.test(buffer)) {
          this.push(buffer + '\n')
        }
        callback()
      },
    })
  }

  /**
   * Cria filtro para modo "data-only" (mantém apenas dados)
   */
  private createDataOnlyFilter(dbType: string): Transform {
    if (dbType === 'postgresql') {
      return this.createPostgresDataOnlyFilter()
    }
    return this.createMysqlDataOnlyFilter()
  }

  /**
   * Filtro data-only para PostgreSQL
   * Mantém apenas blocos COPY e INSERTs, além de SETs e transações
   */
  private createPostgresDataOnlyFilter(): Transform {
    let insideCopyBlock = false
    let buffer = ''

    return new Transform({
      transform(chunk, _encoding, callback) {
        buffer += chunk.toString()
        const lines = buffer.split('\n')
        buffer = lines.pop() || ''

        const output: string[] = []
        for (const line of lines) {
          if (insideCopyBlock) {
            output.push(line)
            if (line === '\\.') {
              insideCopyBlock = false
            }
            continue
          }

          // COPY ... FROM stdin
          if (/^COPY\s+.*\s+FROM\s+stdin/i.test(line)) {
            insideCopyBlock = true
            output.push(line)
            continue
          }

          // INSERT
          if (/^\s*INSERT\s+INTO\s+/i.test(line)) {
            output.push(line)
            continue
          }

          // SET, BEGIN, COMMIT, configurações de sessão
          if (/^\s*(SET|BEGIN|COMMIT|ROLLBACK|SELECT\s+pg_catalog\.set_config)/i.test(line)) {
            output.push(line)
            continue
          }

          // Comentários e linhas vazias (manter para legibilidade)
          if (/^\s*--/.test(line) || line.trim() === '') {
            output.push(line)
            continue
          }

          // Disable/Enable triggers (necessário para COPY)
          if (/^\s*ALTER\s+TABLE\s+.*\s+(DISABLE|ENABLE)\s+TRIGGER/i.test(line)) {
            output.push(line)
            continue
          }
        }

        if (output.length > 0) {
          this.push(output.join('\n') + '\n')
        }
        callback()
      },
      flush(callback) {
        if (buffer) {
          this.push(buffer + '\n')
        }
        callback()
      },
    })
  }

  /**
   * Filtro data-only para MySQL/MariaDB
   * Mantém apenas INSERT INTO, LOCK/UNLOCK TABLES e SETs
   */
  private createMysqlDataOnlyFilter(): Transform {
    let buffer = ''

    return new Transform({
      transform(chunk, _encoding, callback) {
        buffer += chunk.toString()
        const lines = buffer.split('\n')
        buffer = lines.pop() || ''

        const output: string[] = []
        for (const line of lines) {
          // Manter INSERTs
          if (/^\s*INSERT\s+INTO\s+/i.test(line)) {
            output.push(line)
            continue
          }
          // LOCK/UNLOCK (necessários para os INSERTs)
          if (/^\s*(LOCK\s+TABLES|UNLOCK\s+TABLES)/i.test(line)) {
            output.push(line)
            continue
          }
          // SET, desabilitar checks
          if (/^\s*SET\s+/i.test(line) || /^\s*\/\*!\d+\s+SET\s+/i.test(line)) {
            output.push(line)
            continue
          }
          // Comentários e linhas vazias
          if (/^\s*--/.test(line) || /^\s*\/\*/.test(line) || line.trim() === '') {
            output.push(line)
            continue
          }
        }

        if (output.length > 0) {
          this.push(output.join('\n') + '\n')
        }
        callback()
      },
      flush(callback) {
        if (buffer) {
          this.push(buffer + '\n')
        }
        callback()
      },
    })
  }

  /**
   * Cria um filtro genérico que remove linhas que casam com o regex
   */
  private createLineFilter(pattern: RegExp): Transform {
    let buffer = ''

    return new Transform({
      transform(chunk, _encoding, callback) {
        buffer += chunk.toString()
        const lines = buffer.split('\n')
        buffer = lines.pop() || ''

        const output: string[] = []
        for (const line of lines) {
          if (!pattern.test(line)) {
            output.push(line)
          }
        }

        if (output.length > 0) {
          this.push(output.join('\n') + '\n')
        }
        callback()
      },
      flush(callback) {
        if (buffer && !pattern.test(buffer)) {
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
    const processEnv = { ...process.env }

    if (connection.type === 'postgresql') {
      return {
        command: 'psql',
        args: [
          '-h', connection.host,
          '-p', connection.port.toString(),
          '-U', connection.username,
          '-d', targetDatabase,
          '--no-password',
          '-v', 'ON_ERROR_STOP=1',
        ],
        env: { ...processEnv, PGPASSWORD: password },
      }
    }

    // MySQL / MariaDB
    return {
      command: 'mysql',
      args: [
        '-h', connection.host,
        '-P', connection.port.toString(),
        '-u', connection.username,
        `--password=${password}`,
        targetDatabase,
      ],
      env: processEnv,
    }
  }

  /**
   * Executa o processo de restore
   */
  private executeRestore(
    config: RestoreConfig,
    inputStream: Readable,
    databaseName: string
  ): Promise<RestoreResult> {
    return new Promise((resolve) => {
      const restoreProcess = spawn(config.command, config.args, {
        env: config.env,
        stdio: ['pipe', 'pipe', 'pipe'],
      })

      let stderrData = ''
      let stdoutData = ''

      restoreProcess.stdout?.on('data', (data: Buffer) => {
        stdoutData += data.toString()
      })

      restoreProcess.stderr?.on('data', (data: Buffer) => {
        stderrData += data.toString()
      })

      // Erro ao iniciar o processo
      restoreProcess.on('error', (error) => {
        resolve({
          success: false,
          databaseName,
          durationSeconds: 0,
          error: `Falha ao executar ${config.command}: ${error.message}. ` +
            'Verifique se o binário está instalado e no PATH.',
        })
      })

      restoreProcess.on('close', (exitCode) => {
        const warnings = this.extractWarnings(stderrData)

        if (exitCode === 0) {
          resolve({
            success: true,
            databaseName,
            durationSeconds: 0,
            exitCode: 0,
            warnings: warnings.length > 0 ? warnings : undefined,
          })
        } else {
          resolve({
            success: false,
            databaseName,
            durationSeconds: 0,
            error: stderrData || stdoutData || `Processo terminou com código ${exitCode}`,
            exitCode: exitCode ?? undefined,
            warnings: warnings.length > 0 ? warnings : undefined,
          })
        }
      })

      // Pipar o stream de entrada para o stdin do processo
      inputStream.pipe(restoreProcess.stdin!)

      inputStream.on('error', (error) => {
        restoreProcess.stdin?.end()
        resolve({
          success: false,
          databaseName,
          durationSeconds: 0,
          error: `Erro ao ler arquivo de backup: ${error.message}`,
        })
      })

      restoreProcess.stdin?.on('error', (error) => {
        // Ignorar EPIPE (processo já terminou)
        if ((error as NodeJS.ErrnoException).code !== 'EPIPE') {
          logger.warn(`[Restore] Erro no stdin do processo: ${error.message}`)
        }
      })
    })
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
