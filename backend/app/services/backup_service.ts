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
import ConnectionDatabase from '#models/connection_database'
import Backup, { type BackupTrigger, type RetentionType } from '#models/backup'
import { StorageDestinationService } from '#services/storage_destination_service'
import { StorageSpaceService } from '#services/storage_space_service'
import { NotificationService } from '#services/notification_service'

/**
 * Resultado da execução de um backup individual
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
 * Resultado da execução de backups múltiplos
 */
export interface MultiBackupResult {
  totalDatabases: number
  successful: number
  failed: number
  results: Array<{
    databaseName: string
    backup: Backup
    result: BackupResult
  }>
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
 * Agora suporta múltiplos databases por conexão.
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
   * Executa o backup de TODOS os databases habilitados de uma conexão
   */
  async executeAll(
    connection: Connection,
    trigger: BackupTrigger = 'manual'
  ): Promise<MultiBackupResult> {
    // Carregar databases habilitados
    const databases = await connection.getEnabledDatabases()

    if (databases.length === 0) {
      logger.warn(
        `[Backup] Conexão "${connection.name}" (ID: ${connection.id}) não possui databases habilitados para backup`
      )
      return {
        totalDatabases: 0,
        successful: 0,
        failed: 0,
        results: [],
      }
    }

    logger.info(
      `[Backup] Iniciando backup de ${databases.length} database(s) da conexão "${connection.name}" (ID: ${connection.id})`
    )

    const results: MultiBackupResult['results'] = []

    // Executar backup de cada database
    for (const connDb of databases) {
      const { backup, result } = await this.executeForDatabase(connection, connDb, trigger)
      results.push({
        databaseName: connDb.databaseName,
        backup,
        result,
      })
    }

    const successful = results.filter((r) => r.result.success).length
    const failed = results.filter((r) => !r.result.success).length

    logger.info(
      `[Backup] Finalizado backup da conexão "${connection.name}": ` +
        `${successful} sucesso, ${failed} falha(s) de ${databases.length} database(s)`
    )

    // Atualizar último backup da conexão
    connection.lastBackupAt = DateTime.now()
    await connection.save()

    return {
      totalDatabases: databases.length,
      successful,
      failed,
      results,
    }
  }

  /**
   * Executa o backup de UM database específico
   */
  async executeForDatabase(
    connection: Connection,
    connDb: ConnectionDatabase,
    trigger: BackupTrigger = 'manual'
  ): Promise<{ backup: Backup; result: BackupResult }> {
    const destination = await StorageDestinationService.resolveDestinationForConnection(connection)
    const destinationName = destination?.name ?? 'Local (padrão)'

    // Verificar espaço em disco antes do backup
    const spaceCheck = await StorageSpaceService.checkSpaceBeforeBackup(destination)

    // Criar e salvar registro inicial do backup
    const backup = await this.createBackupRecord(connection, connDb, destination, trigger)

    logger.info(
      `[Backup] Iniciado backup do database "${connDb.databaseName}" da conexão "${connection.name}" ` +
        `(ID: ${connection.id}) para "${destinationName}" - Backup ID: ${backup.id}`
    )

    // Envia notificação de backup iniciado
    NotificationService.backupStarted(
      `${connection.name} / ${connDb.databaseName}`,
      connection.id,
      trigger
    )

    // Envia notificação de espaço baixo se aplicável
    if (spaceCheck.warning) {
      const spaceInfo = await StorageSpaceService.getDestinationSpaceInfo(destination)
      if (spaceInfo) {
        NotificationService.storageSpaceLow(
          spaceInfo.destinationName,
          spaceInfo.freePercent,
          spaceInfo.freeBytes,
          destination?.id
        )
      }
    }

    try {
      const localBasePath = StorageDestinationService.getLocalBasePath(destination)
      const result = await this.performBackup(connection, connDb.databaseName, localBasePath)

      // Adicionar aviso de espaço baixo ao resultado, se houver
      if (spaceCheck.warning) {
        result.storageWarning = spaceCheck.warning
      }

      // Upload para destino remoto, se configurado
      if (result.success && destination) {
        await this.uploadToRemoteDestination(destination, result)
      }

      // Atualizar registro do backup com o resultado
      await this.updateBackupRecord(backup, connection, connDb, result)

      return { backup, result }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Erro desconhecido'
      return await this.handleBackupError(backup, connection, connDb, errorMessage)
    }
  }

  /**
   * Executa backup de um único database (método legado para compatibilidade)
   * @deprecated Use executeForDatabase ou executeAll
   */
  async execute(
    connection: Connection,
    trigger: BackupTrigger = 'manual'
  ): Promise<{ backup: Backup; result: BackupResult }> {
    // Buscar o primeiro database habilitado
    const databases = await connection.getEnabledDatabases()
    
    if (databases.length === 0) {
      // Criar backup de erro se não houver databases
      const backup = new Backup()
      backup.connectionId = connection.id
      backup.databaseName = 'N/A'
      backup.trigger = trigger
      backup.compressed = true
      backup.retentionType = this.determineRetentionType()
      backup.markAsStarted()
      backup.markAsFailed('Nenhum database habilitado para backup')
      await backup.save()

      return {
        backup,
        result: {
          success: false,
          error: 'Nenhum database habilitado para backup',
        },
      }
    }

    return this.executeForDatabase(connection, databases[0], trigger)
  }

  /**
   * Cria o registro inicial do backup no banco de dados
   */
  private async createBackupRecord(
    connection: Connection,
    connDb: ConnectionDatabase,
    destination: Awaited<ReturnType<typeof StorageDestinationService.resolveDestinationForConnection>>,
    trigger: BackupTrigger
  ): Promise<Backup> {
    const backup = new Backup()
    backup.connectionId = connection.id
    backup.connectionDatabaseId = connDb.id
    backup.databaseName = connDb.databaseName
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
    connDb: ConnectionDatabase,
    result: BackupResult
  ): Promise<void> {
    if (result.success) {
      backup.markAsCompleted(result.filePath!, result.fileName!, result.fileSize!, result.checksum)

      const fileSizeKb = result.fileSize ? (result.fileSize / 1024).toFixed(2) : '0'
      logger.info(
        `[Backup] Concluído backup do database "${connDb.databaseName}" da conexão "${connection.name}" ` +
          `- Backup ID: ${backup.id}, Arquivo: ${result.fileName}, Tamanho: ${fileSizeKb} KB`
      )

      // Envia notificação de backup concluído
      NotificationService.backupCompleted(
        `${connection.name} / ${connDb.databaseName}`,
        connection.id,
        backup.id,
        result.fileName!,
        result.fileSize!
      )
    } else {
      this.copyPartialResultToBackup(backup, result)
      backup.markAsFailed(result.error ?? 'Erro desconhecido', result.exitCode)

      logger.error(
        `[Backup] Falhou backup do database "${connDb.databaseName}" da conexão "${connection.name}" ` +
          `- Backup ID: ${backup.id}, Erro: ${result.error ?? 'Erro desconhecido'}`
      )

      // Envia notificação de backup falhou
      NotificationService.backupFailed(
        `${connection.name} / ${connDb.databaseName}`,
        connection.id,
        result.error ?? 'Erro desconhecido'
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
    connDb: ConnectionDatabase,
    errorMessage: string
  ): Promise<{ backup: Backup; result: BackupResult }> {
    backup.markAsFailed(errorMessage)
    await backup.save()

    logger.error(
      `[Backup] Erro inesperado no backup do database "${connDb.databaseName}" ` +
        `da conexão "${connection.name}" (ID: ${connection.id}) - Backup ID: ${backup.id}, Erro: ${errorMessage}`
    )

    return {
      backup,
      result: { success: false, error: errorMessage },
    }
  }

  /**
   * Executa o comando de dump do banco de dados
   */
  private async performBackup(
    connection: Connection,
    databaseName: string,
    basePath: string
  ): Promise<BackupResult> {
    const { fileName, relativePath, fullPath } = this.buildFilePaths(connection, databaseName, basePath)

    // Garantir que o diretório da conexão existe
    this.ensureDirectoryExists(join(basePath, connection.id.toString()))

    const dumpConfig = this.buildDumpConfig(connection, databaseName)

    return this.executeDumpProcess(dumpConfig, fullPath, relativePath, fileName)
  }

  /**
   * Constrói os caminhos de arquivo para o backup
   */
  private buildFilePaths(
    connection: Connection,
    databaseName: string,
    basePath: string
  ): {
    fileName: string
    relativePath: string
    fullPath: string
  } {
    const timestamp = DateTime.now().toFormat('yyyyMMdd_HHmmss')
    const fileName = `${databaseName}_${timestamp}.sql.gz`
    const relativePath = join(connection.id.toString(), fileName)
    const fullPath = join(basePath, relativePath)

    return { fileName, relativePath, fullPath }
  }

  /**
   * Constrói a configuração do comando de dump baseado no tipo de banco
   */
  private buildDumpConfig(connection: Connection, databaseName: string): DumpConfig {
    const password = connection.getDecryptedPassword()
    const processEnv = { ...process.env }

    if (connection.type === 'postgresql') {
      return this.buildPostgresConfig(connection, databaseName, password, processEnv)
    }

    return this.buildMySqlConfig(connection, databaseName, password, processEnv)
  }

  /**
   * Configuração para PostgreSQL (pg_dump)
   */
  private buildPostgresConfig(
    connection: Connection,
    databaseName: string,
    password: string,
    env: NodeJS.ProcessEnv
  ): DumpConfig {
    return {
      command: 'pg_dump',
      args: [
        '-h', connection.host,
        '-p', connection.port.toString(),
        '-U', connection.username,
        '-d', databaseName,
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
    databaseName: string,
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
        databaseName,
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
