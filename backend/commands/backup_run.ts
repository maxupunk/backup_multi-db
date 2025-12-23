import { BaseCommand } from '@adonisjs/core/ace'
import type { CommandOptions } from '@adonisjs/core/types/ace'
import Connection from '#models/connection'
import { BackupService } from '#services/backup_service'
import app from '@adonisjs/core/services/app'

export default class BackupRun extends BaseCommand {
  static commandName = 'backup:run'
  static description = 'Run a backup for a specific connection manually'

  static options: CommandOptions = {
    startApp: true,
  }

  async run() {
    const backupService = await app.container.make(BackupService)

    try {
      const connections = await Connection.query().preload('databases', (query) => {
        query.where('enabled', true)
      })

      if (connections.length === 0) {
        this.logger.error('No connections found. Please create a connection first.')
        return
      }

      const choices = connections.map((conn) => {
        const dbCount = conn.databases?.length ?? 0
        return {
          name: conn.id.toString(),
          message: `${conn.name} (${dbCount} database(s) @ ${conn.host})`,
        }
      })

      const selectedId = await this.prompt.choice(
        'Select a connection to backup',
        choices
      )

      const connection = connections.find((c) => c.id.toString() === selectedId)

      if (!connection) {
        this.logger.error('Connection not found')
        return
      }

      const databaseCount = await connection.getEnabledDatabasesCount()
      if (databaseCount === 0) {
        this.logger.error('No databases enabled for backup in this connection.')
        return
      }

      this.logger.info(`Starting backup for "${connection.name}" (${databaseCount} database(s))...`)

      const result = await backupService.executeAll(connection, 'manual')

      this.logger.info(`Backup completed: ${result.successful} success, ${result.failed} failed`)

      for (const r of result.results) {
        if (r.result.success) {
          this.logger.success(`[${r.databaseName}] File: ${r.result.fileName}`)
          this.logger.info(`  Size: ${(r.result.fileSize! / 1024 / 1024).toFixed(2)} MB`)
        } else {
          this.logger.error(`[${r.databaseName}] Failed: ${r.result.error}`)
        }
      }
    } catch (error) {
      this.logger.error(`An error occurred: ${error.message}`)
      this.error = error
      this.exitCode = 1
    }
  }
}