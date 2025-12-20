import { spawn } from 'node:child_process'
import { createWriteStream, existsSync, mkdirSync, statSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { pipeline } from 'node:stream/promises'
import { createGzip } from 'node:zlib'
import { join } from 'node:path'
import { DateTime } from 'luxon'
import app from '@adonisjs/core/services/app'
import Connection from '#models/connection'
import Backup, { type BackupTrigger, type RetentionType } from '#models/backup'

/**
 * Resultado da execução de um backup
 */
export interface BackupResult {
  success: boolean
  filePath?: string
  fileName?: string
  fileSize?: number
  checksum?: string
  error?: string
  exitCode?: number
}

/**
 * Serviço responsável por executar backups de banco de dados
 */
export class BackupService {
  private readonly storagePath: string

  constructor() {
    this.storagePath = app.makePath('storage/backups')
    this.ensureStorageDirectory()
  }

  /**
   * Garante que o diretório de armazenamento existe
   */
  private ensureStorageDirectory(): void {
    if (!existsSync(this.storagePath)) {
      mkdirSync(this.storagePath, { recursive: true })
    }
  }

  /**
   * Executa o backup de uma conexão
   */
  async execute(
    connection: Connection,
    trigger: BackupTrigger = 'manual'
  ): Promise<{ backup: Backup; result: BackupResult }> {
    // Criar registro do backup
    const backup = new Backup()
    backup.connectionId = connection.id
    backup.trigger = trigger
    backup.compressed = true
    backup.retentionType = this.determineRetentionType()
    backup.markAsStarted()
    await backup.save()

    try {
      const result = await this.performBackup(connection)

      if (result.success) {
        backup.markAsCompleted(
          result.filePath!,
          result.fileName!,
          result.fileSize!,
          result.checksum
        )

        // Atualizar última data de backup na conexão
        connection.lastBackupAt = DateTime.now()
        await connection.save()
      } else {
        backup.markAsFailed(result.error ?? 'Erro desconhecido', result.exitCode)
      }

      await backup.save()

      return { backup, result }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Erro desconhecido'
      backup.markAsFailed(errorMessage)
      await backup.save()

      return {
        backup,
        result: { success: false, error: errorMessage },
      }
    }
  }

  /**
   * Executa o comando de backup real
   */
  private async performBackup(connection: Connection): Promise<BackupResult> {
    const timestamp = DateTime.now().toFormat('yyyyMMdd_HHmmss')
    const fileName = `${connection.database}_${timestamp}.sql.gz`
    const relativePath = join(connection.id.toString(), fileName)
    const fullPath = join(this.storagePath, relativePath)

    // Criar diretório da conexão se não existir
    const connectionDir = join(this.storagePath, connection.id.toString())
    if (!existsSync(connectionDir)) {
      mkdirSync(connectionDir, { recursive: true })
    }

    const password = connection.getDecryptedPassword()

    return new Promise((resolve) => {
      let command: string
      let args: string[]
      const env = { ...process.env }

      if (connection.type === 'postgresql') {
        // PostgreSQL - pg_dump
        command = 'pg_dump'
        args = [
          '-h',
          connection.host,
          '-p',
          connection.port.toString(),
          '-U',
          connection.username,
          '-d',
          connection.database,
          '--no-password',
        ]
        // pg_dump usa PGPASSWORD para senha
        env.PGPASSWORD = password
      } else {
        // MySQL / MariaDB - mysqldump
        command = 'mysqldump'
        args = [
          '-h',
          connection.host,
          '-P',
          connection.port.toString(),
          '-u',
          connection.username,
          `--password=${password}`,
          '--single-transaction',
          '--routines',
          '--triggers',
          connection.database,
        ]
      }

      const dumpProcess = spawn(command, args, {
        env,
        stdio: ['ignore', 'pipe', 'pipe'],
      })

      const gzip = createGzip()
      const outputStream = createWriteStream(fullPath)
      const hash = createHash('sha256')

      let stderrData = ''

      dumpProcess.stderr.on('data', (data: Buffer) => {
        stderrData += data.toString()
      })

      // Pipeline: dump stdout -> gzip -> arquivo
      // Também calcular hash durante o stream
      dumpProcess.stdout.on('data', (data: Buffer) => {
        hash.update(data)
      })

      pipeline(dumpProcess.stdout, gzip, outputStream)
        .then(() => {
          dumpProcess.on('close', (code) => {
            if (code === 0) {
              const stats = statSync(fullPath)
              resolve({
                success: true,
                filePath: relativePath,
                fileName: fileName,
                fileSize: stats.size,
                checksum: hash.digest('hex'),
              })
            } else {
              resolve({
                success: false,
                error: stderrData || `Processo terminou com código ${code}`,
                exitCode: code ?? undefined,
              })
            }
          })
        })
        .catch((error) => {
          resolve({
            success: false,
            error: error instanceof Error ? error.message : 'Erro no pipeline',
          })
        })

      dumpProcess.on('error', (error) => {
        resolve({
          success: false,
          error: `Falha ao executar ${command}: ${error.message}. Verifique se o binário está instalado e no PATH.`,
        })
      })
    })
  }

  /**
   * Determina o tipo de retenção baseado no momento atual
   */
  private determineRetentionType(): RetentionType {
    const now = DateTime.now()

    // Verificar se é o último dia do ano
    if (now.month === 12 && now.day === 31) {
      return 'yearly'
    }

    // Verificar se é o último dia do mês
    if (now.day === now.daysInMonth) {
      return 'monthly'
    }

    // Verificar se é domingo (fim da semana)
    if (now.weekday === 7) {
      return 'weekly'
    }

    // Verificar se é o último backup do dia (23h)
    if (now.hour >= 23) {
      return 'daily'
    }

    return 'hourly'
  }
}
