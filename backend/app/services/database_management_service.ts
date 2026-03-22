import { spawn } from 'node:child_process'
import logger from '@adonisjs/core/services/logger'
import Connection from '#models/connection'

/**
 * Resultado de uma operação de gerenciamento de banco de dados
 */
export interface DatabaseOperationResult {
  success: boolean
  error?: string
}

/**
 * Serviço responsável por operações de gerenciamento de bancos de dados
 * (verificar existência, criar, etc.) via comandos nativos do SGBD.
 * Usa spawn com array de args — sem interpolação shell, sem risco de injeção.
 */
export class DatabaseManagementService {
  /**
   * Verifica se um banco de dados existe na conexão especificada
   */
  async databaseExists(connection: Connection, databaseName: string): Promise<boolean> {
    const { command, args, env } = this.buildCheckCommand(connection, databaseName)

    return new Promise((resolve) => {
      const proc = spawn(command, args, { env, stdio: ['ignore', 'pipe', 'pipe'] })
      proc.on('error', () => resolve(false))
      proc.on('close', (code) => resolve(code === 0))
    })
  }

  /**
   * Cria um banco de dados na conexão especificada.
   * Retorna sucesso ou o erro produzido pelo SGBD.
   */
  async createDatabase(
    connection: Connection,
    databaseName: string
  ): Promise<DatabaseOperationResult> {
    const { command, args, env } = this.buildCreateCommand(connection, databaseName)

    logger.info(
      `[DatabaseManagement] Criando database "${databaseName}" na conexão "${connection.name}" (${connection.type})`
    )

    return new Promise((resolve) => {
      let stderr = ''

      const proc = spawn(command, args, { env, stdio: ['ignore', 'pipe', 'pipe'] })

      proc.stderr.on('data', (chunk: Buffer) => {
        stderr += chunk.toString()
      })

      proc.on('error', (err) => {
        logger.error(`[DatabaseManagement] Erro ao criar database "${databaseName}": ${err.message}`)
        resolve({ success: false, error: err.message })
      })

      proc.on('close', (code) => {
        if (code === 0) {
          logger.info(`[DatabaseManagement] Database "${databaseName}" criado com sucesso`)
          resolve({ success: true })
        } else {
          const errorMsg = stderr.trim() || `Processo encerrou com código ${code}`
          logger.error(`[DatabaseManagement] Falha ao criar database "${databaseName}": ${errorMsg}`)
          resolve({ success: false, error: errorMsg })
        }
      })
    })
  }

  // ─── Privados ──────────────────────────────────────────────────────────────

  private buildCheckCommand(
    connection: Connection,
    databaseName: string
  ): { command: string; args: string[]; env: NodeJS.ProcessEnv } {
    const password = connection.getDecryptedPassword()

    if (connection.type === 'postgresql') {
      return {
        command: 'psql',
        args: [
          '-h', connection.host,
          '-p', connection.port.toString(),
          '-U', connection.username,
          '-d', databaseName,
          '--no-password',
          '--tuples-only',
          '--quiet',
          '-c', 'SELECT 1',
        ],
        env: { ...process.env, PGPASSWORD: password },
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
        '-e', 'SELECT 1',
        databaseName,
      ],
      env: { ...process.env },
    }
  }

  private buildCreateCommand(
    connection: Connection,
    databaseName: string
  ): { command: string; args: string[]; env: NodeJS.ProcessEnv } {
    const password = connection.getDecryptedPassword()

    if (connection.type === 'postgresql') {
      return {
        command: 'psql',
        args: [
          '-h', connection.host,
          '-p', connection.port.toString(),
          '-U', connection.username,
          '--no-password',
          '-c', `CREATE DATABASE "${databaseName}"`,
        ],
        env: { ...process.env, PGPASSWORD: password },
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
        '-e', `CREATE DATABASE \`${databaseName}\``,
      ],
      env: { ...process.env },
    }
  }
}
