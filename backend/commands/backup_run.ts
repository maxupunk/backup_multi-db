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
      const connections = await Connection.all()

      if (connections.length === 0) {
        this.logger.error('No connections found. Please create a connection first.')
        return
      }

      const choices = connections.map((conn) => ({
        name: conn.id.toString(),
        message: `${conn.name} (${conn.database} @ ${conn.host})`,
      }))

      const selectedId = await this.prompt.choice(
        'Select a connection to backup',
        choices
      )

      const connection = connections.find((c) => c.id.toString() === selectedId)

      if (!connection) {
        this.logger.error('Connection not found')
        return
      }

      this.logger.info(`Starting backup for "${connection.name}"...`)

      const { result } = await backupService.execute(connection, 'manual')

      if (result.success) {
        this.logger.success(`Backup completed successfully!`)
        this.logger.info(`File: ${result.fileName}`)
        this.logger.info(`Size: ${(result.fileSize! / 1024 / 1024).toFixed(2)} MB`)
        this.logger.info(`Path: ${result.localFullPath}`)
      } else {
        this.logger.error(`Backup failed: ${result.error}`)
        if (result.exitCode) {
            this.logger.error(`Exit Code: ${result.exitCode}`)
        }
      }
    } catch (error) {
      this.logger.error(`An error occurred: ${error.message}`)
      this.error = error
      this.exitCode = 1
    }
  }
}