import { spawn, type ChildProcess } from 'node:child_process'
import { createWriteStream, existsSync, mkdirSync, statSync } from 'node:fs'
import { createHash, type Hash } from 'node:crypto'
import { createGzip, type Gzip } from 'node:zlib'
import { join } from 'node:path'
import type { Readable } from 'node:stream'
import { DateTime } from 'luxon'
import app from '@adonisjs/core/services/app'
import env from '#start/env'
import logger from '@adonisjs/core/services/logger'
import Connection from '#models/connection'
import Backup, { type BackupTrigger, type RetentionType } from '#models/backup'
import { StorageDestinationService } from '#services/storage_destination_service'
import { StorageSpaceService } from '#services/storage_space_service'

/**
 * Resultado da execução de um backup
 */
export interface BackupResult {
  success: boolean
  filePath?: string
  localFullPath?: string
  fileName?: string
  fileSize?: number
  checksum?: string
  error?: string
  exitCode?: number
  storageWarning?: string
}

/**
 * Configuração do comando de dump
 */
interface DumpConfig {
  command: string
  args: string[]
  env: NodeJS.ProcessEnv
}

/**
 * Serviço responsável por executar backups de banco de dados.
 * Suporta MySQL, MariaDB e PostgreSQL.
 */
export class BackupService {
  private readonly storagePath: string

  constructor() {
    this.storagePath = env.get('BACKUP_STORAGE_PATH') ?? app.makePath('storage/backups')
    this.ensureDirectoryExists(this.storagePath)
  }

  /**
   * Garante que um diretório existe, criando-o se necessário
   */
  private ensureDirectoryExists(path: string): void {
    if (!existsSync(path)) {
      mkdirSync(path, { recursive: true })
    }
  }

  /**
   * Executa o backup de uma conexão
   */
  async execute(
    connection: Connection,
    trigger: BackupTrigger = 'manual'
  ): Promise<{ backup: Backup; result: BackupResult }> {
    const destination = await StorageDestinationService.resolveDestinationForConnection(connection)
    const destinationName = destination?.name ?? 'Local (padrão)'

    // Verificar espaço em disco antes do backup
    const spaceCheck = await StorageSpaceService.checkSpaceBeforeBackup(destination)

    // Criar e salvar registro inicial do backup
    const backup = await this.createBackupRecord(connection, destination, trigger)

    logger.info(
      `[Backup] Iniciado backup da conexão "${connection.name}" (ID: ${connection.id}) ` +
        `para o armazenamento "${destinationName}" - Backup ID: ${backup.id}, Trigger: ${trigger}`
    )

    try {
      const localBasePath = StorageDestinationService.getLocalBasePath(destination)
      const result = await this.performBackup(connection, localBasePath)

      // Adicionar aviso de espaço baixo ao resultado, se houver
      if (spaceCheck.warning) {
        result.storageWarning = spaceCheck.warning
      }

      // Upload para destino remoto, se configurado
      if (result.success && destination) {
        await this.uploadToRemoteDestination(destination, result)
      }

      // Atualizar registro do backup com o resultado
      await this.updateBackupRecord(backup, connection, result)

      return { backup, result }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Erro desconhecido'
      return await this.handleBackupError(backup, connection, errorMessage)
    }
  }

  /**
   * Cria o registro inicial do backup no banco de dados
   */
  private async createBackupRecord(
    connection: Connection,
    destination: Awaited<ReturnType<typeof StorageDestinationService.resolveDestinationForConnection>>,
    trigger: BackupTrigger
  ): Promise<Backup> {
    const backup = new Backup()
    backup.connectionId = connection.id
    backup.storageDestinationId = destination?.id ?? null
    backup.trigger = trigger
    backup.compressed = true
    backup.retentionType = this.determineRetentionType()
    backup.markAsStarted()
    await backup.save()
    return backup
  }

  /**
   * Tenta fazer upload do backup para o destino remoto
   */
  private async uploadToRemoteDestination(
    destination: NonNullable<
      Awaited<ReturnType<typeof StorageDestinationService.resolveDestinationForConnection>>
    >,
    result: BackupResult
  ): Promise<void> {
    try {
      await StorageDestinationService.uploadBackupFile(
        destination,
        result.filePath!,
        result.localFullPath!
      )
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Erro desconhecido no upload'
      result.success = false
      result.error = errorMessage
    }
  }

  /**
   * Atualiza o registro do backup com o resultado da operação
   */
  private async updateBackupRecord(
    backup: Backup,
    connection: Connection,
    result: BackupResult
  ): Promise<void> {
    if (result.success) {
      backup.markAsCompleted(result.filePath!, result.fileName!, result.fileSize!, result.checksum)
      connection.lastBackupAt = DateTime.now()
      await connection.save()

      const fileSizeKb = result.fileSize ? (result.fileSize / 1024).toFixed(2) : '0'
      logger.info(
        `[Backup] Concluído backup da conexão "${connection.name}" (ID: ${connection.id}) ` +
          `- Backup ID: ${backup.id}, Arquivo: ${result.fileName}, Tamanho: ${fileSizeKb} KB`
      )
    } else {
      this.copyPartialResultToBackup(backup, result)
      backup.markAsFailed(result.error ?? 'Erro desconhecido', result.exitCode)

      logger.error(
        `[Backup] Falhou backup da conexão "${connection.name}" (ID: ${connection.id}) ` +
          `- Backup ID: ${backup.id}, Erro: ${result.error ?? 'Erro desconhecido'}`
      )
    }

    await backup.save()
  }

  /**
   * Copia dados parciais do resultado para o registro de backup em caso de falha
   */
  private copyPartialResultToBackup(backup: Backup, result: BackupResult): void {
    if (result.filePath) backup.filePath = result.filePath
    if (result.fileName) backup.fileName = result.fileName
    if (result.fileSize !== undefined) backup.fileSize = result.fileSize
    if (result.checksum) backup.checksum = result.checksum
  }

  /**
   * Trata erros inesperados durante o backup
   */
  private async handleBackupError(
    backup: Backup,
    connection: Connection,
    errorMessage: string
  ): Promise<{ backup: Backup; result: BackupResult }> {
    backup.markAsFailed(errorMessage)
    await backup.save()

    logger.error(
      `[Backup] Erro inesperado no backup da conexão "${connection.name}" (ID: ${connection.id}) ` +
        `- Backup ID: ${backup.id}, Erro: ${errorMessage}`
    )

    return {
      backup,
      result: { success: false, error: errorMessage },
    }
  }

  /**
   * Executa o comando de dump do banco de dados
   */
  private async performBackup(connection: Connection, basePath: string): Promise<BackupResult> {
    const { fileName, relativePath, fullPath } = this.buildFilePaths(connection, basePath)

    // Garantir que o diretório da conexão existe
    this.ensureDirectoryExists(join(basePath, connection.id.toString()))

    const dumpConfig = this.buildDumpConfig(connection)

    return this.executeDumpProcess(dumpConfig, fullPath, relativePath, fileName)
  }

  /**
   * Constrói os caminhos de arquivo para o backup
   */
  private buildFilePaths(
    connection: Connection,
    basePath: string
  ): {
    fileName: string
    relativePath: string
    fullPath: string
  } {
    const timestamp = DateTime.now().toFormat('yyyyMMdd_HHmmss')
    const fileName = `${connection.database}_${timestamp}.sql.gz`
    const relativePath = join(connection.id.toString(), fileName)
    const fullPath = join(basePath, relativePath)

    return { fileName, relativePath, fullPath }
  }

  /**
   * Constrói a configuração do comando de dump baseado no tipo de banco
   */
  private buildDumpConfig(connection: Connection): DumpConfig {
    const password = connection.getDecryptedPassword()
    const processEnv = { ...process.env }

    if (connection.type === 'postgresql') {
      return this.buildPostgresConfig(connection, password, processEnv)
    }

    return this.buildMySqlConfig(connection, password, processEnv)
  }

  /**
   * Configuração para PostgreSQL (pg_dump)
   */
  private buildPostgresConfig(
    connection: Connection,
    password: string,
    env: NodeJS.ProcessEnv
  ): DumpConfig {
    return {
      command: 'pg_dump',
      args: [
        '-h', connection.host,
        '-p', connection.port.toString(),
        '-U', connection.username,
        '-d', connection.database,
        '--no-password',
      ],
      env: { ...env, PGPASSWORD: password },
    }
  }

  /**
   * Configuração para MySQL/MariaDB (mysqldump)
   */
  private buildMySqlConfig(
    connection: Connection,
    password: string,
    env: NodeJS.ProcessEnv
  ): DumpConfig {
    return {
      command: 'mysqldump',
      args: [
        '-h', connection.host,
        '-P', connection.port.toString(),
        '-u', connection.username,
        `--password=${password}`,
        '--single-transaction',
        '--routines',
        '--triggers',
        connection.database,
      ],
      env,
    }
  }

  /**
   * Executa o processo de dump e gerencia os streams
   */
  private executeDumpProcess(
    config: DumpConfig,
    fullPath: string,
    relativePath: string,
    fileName: string
  ): Promise<BackupResult> {
    return new Promise((resolve) => {
      const dumpProcess = spawn(config.command, config.args, {
        env: config.env,
        stdio: ['ignore', 'pipe', 'pipe'],
      })

      const gzip = createGzip()
      const outputStream = createWriteStream(fullPath)
      const hash = createHash('sha256')
      let stderrData = ''

      // Configurar streams
      this.setupStreams(dumpProcess, gzip, outputStream, hash, (data) => {
        stderrData += data
      })

      // Configurar handlers de eventos
      this.setupEventHandlers(
        dumpProcess,
        gzip,
        outputStream,
        hash,
        { fullPath, relativePath, fileName, stderrData: () => stderrData, command: config.command },
        resolve
      )
    })
  }

  /**
   * Configura os streams de dados (stdout -> gzip -> arquivo)
   */
  private setupStreams(
    dumpProcess: ChildProcess,
    gzip: Gzip,
    outputStream: ReturnType<typeof createWriteStream>,
    hash: Hash,
    onStderr: (data: string) => void
  ): void {
    const stdout = dumpProcess.stdout as Readable
    const stderr = dumpProcess.stderr as Readable

    // Processar dados do stdout: calcular hash e comprimir
    stdout.on('data', (data: Buffer) => {
      hash.update(data)
      gzip.write(data)
    })

    // Finalizar gzip quando stdout terminar
    stdout.on('end', () => {
      gzip.end()
    })

    // Conectar gzip ao arquivo de saída
    gzip.pipe(outputStream)

    // Capturar erros do stderr
    stderr.on('data', (data: Buffer) => {
      onStderr(data.toString())
    })
  }

  /**
   * Configura os handlers de eventos para resolver a Promise
   */
  private setupEventHandlers(
    dumpProcess: ChildProcess,
    gzip: Gzip,
    outputStream: ReturnType<typeof createWriteStream>,
    hash: Hash,
    context: {
      fullPath: string
      relativePath: string
      fileName: string
      stderrData: () => string
      command: string
    },
    resolve: (result: BackupResult) => void
  ): void {
    // Erro ao iniciar o processo
    dumpProcess.on('error', (error) => {
      resolve({
        success: false,
        error: `Falha ao executar ${context.command}: ${error.message}. ` +
          'Verifique se o binário está instalado e no PATH.',
      })
    })

    // Arquivo finalizado - verificar resultado
    outputStream.on('finish', () => {
      if (dumpProcess.exitCode === 0) {
        resolve(this.buildSuccessResult(context, hash))
      } else if (dumpProcess.exitCode !== null) {
        resolve({
          success: false,
          error: context.stderrData() || `Processo terminou com código ${dumpProcess.exitCode}`,
          exitCode: dumpProcess.exitCode,
        })
      }
    })

    // Erros nos streams
    outputStream.on('error', (error) => {
      resolve({ success: false, error: `Erro ao escrever arquivo: ${error.message}` })
    })

    gzip.on('error', (error) => {
      resolve({ success: false, error: `Erro na compressão: ${error.message}` })
    })
  }

  /**
   * Constrói o resultado de sucesso com estatísticas do arquivo
   */
  private buildSuccessResult(
    context: { fullPath: string; relativePath: string; fileName: string },
    hash: Hash
  ): BackupResult {
    try {
      const stats = statSync(context.fullPath)
      return {
        success: true,
        filePath: context.relativePath,
        localFullPath: context.fullPath,
        fileName: context.fileName,
        fileSize: stats.size,
        checksum: hash.digest('hex'),
      }
    } catch (err) {
      return {
        success: false,
        error: `Erro ao ler arquivo: ${err instanceof Error ? err.message : 'desconhecido'}`,
      }
    }
  }

  /**
   * Determina o tipo de retenção baseado no momento atual.
   * Usado para políticas de backup (GFS - Grandfather-Father-Son).
   */
  private determineRetentionType(): RetentionType {
    const now = DateTime.now()

    // Último dia do ano -> backup anual
    if (now.month === 12 && now.day === 31) {
      return 'yearly'
    }

    // Último dia do mês -> backup mensal
    if (now.day === now.daysInMonth) {
      return 'monthly'
    }

    // Domingo -> backup semanal
    if (now.weekday === 7) {
      return 'weekly'
    }

    // Após 23h -> backup diário
    if (now.hour >= 23) {
      return 'daily'
    }

    return 'hourly'
  }
}
