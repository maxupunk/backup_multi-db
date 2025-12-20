import cron from 'node-cron'
import Connection, { type ScheduleFrequency } from '#models/connection'
import { BackupService } from '#services/backup_service'
import { RetentionService } from '#services/retention_service'
import logger from '@adonisjs/core/services/logger'

/**
 * Job agendado para backup
 */
interface ScheduledJob {
  connectionId: number
  task: cron.ScheduledTask
  frequency: ScheduleFrequency
}

/**
 * Serviço de agendamento de backups automáticos
 */
export class SchedulerService {
  private jobs: Map<number, ScheduledJob> = new Map()
  private retentionTask: cron.ScheduledTask | null = null
  private backupService: BackupService
  private retentionService: RetentionService
  private isRunning = false

  constructor() {
    this.backupService = new BackupService()
    this.retentionService = new RetentionService()
  }

  /**
   * Inicia o serviço de agendamento
   */
  async start(): Promise<void> {
    if (this.isRunning) {
      logger.warn('Scheduler já está em execução')
      return
    }

    logger.info('Iniciando serviço de agendamento de backups...')

    // Carregar todas as conexões com agendamento ativo
    await this.loadScheduledConnections()

    // Agendar job de retenção (executa diariamente às 2h da manhã)
    this.scheduleRetentionJob()

    this.isRunning = true
    logger.info('Serviço de agendamento iniciado com sucesso')
  }

  /**
   * Para o serviço de agendamento
   */
  stop(): void {
    if (!this.isRunning) {
      logger.warn('Scheduler não está em execução')
      return
    }

    logger.info('Parando serviço de agendamento...')

    // Parar todos os jobs de backup
    for (const [connectionId, job] of this.jobs.entries()) {
      job.task.stop()
      logger.info(`Job de backup da conexão ${connectionId} parado`)
    }
    this.jobs.clear()

    // Parar job de retenção
    if (this.retentionTask) {
      this.retentionTask.stop()
      this.retentionTask = null
      logger.info('Job de retenção parado')
    }

    this.isRunning = false
    logger.info('Serviço de agendamento parado')
  }

  /**
   * Carrega todas as conexões com agendamento ativo
   */
  private async loadScheduledConnections(): Promise<void> {
    const connections = await Connection.query()
      .where('scheduleEnabled', true)
      .whereNotNull('scheduleFrequency')
      .where('status', 'active')

    logger.info(`Carregando ${connections.length} conexões agendadas`)

    for (const connection of connections) {
      await this.scheduleConnection(connection)
    }
  }

  /**
   * Agenda backups para uma conexão
   */
  async scheduleConnection(connection: Connection): Promise<void> {
    if (!connection.scheduleEnabled || !connection.scheduleFrequency) {
      logger.warn(
        `Tentativa de agendar conexão ${connection.id} sem agendamento ativo`
      )
      return
    }

    // Remover job existente se houver
    this.unscheduleConnection(connection.id)

    const cronExpression = this.getCronExpression(connection.scheduleFrequency)

    const task = cron.schedule(
      cronExpression,
      async () => {
        await this.executeBackupJob(connection.id)
      },
      {
        timezone: 'America/Sao_Paulo', // Ajuste conforme necessário
      }
    )

    this.jobs.set(connection.id, {
      connectionId: connection.id,
      task,
      frequency: connection.scheduleFrequency,
    })

    logger.info(
      `Backup agendado para conexão ${connection.id} (${connection.name}) - Frequência: ${connection.scheduleFrequency}`
    )
  }

  /**
   * Remove agendamento de uma conexão
   */
  unscheduleConnection(connectionId: number): void {
    const job = this.jobs.get(connectionId)

    if (job) {
      job.task.stop()
      this.jobs.delete(connectionId)
      logger.info(`Job de backup da conexão ${connectionId} removido`)
    }
  }

  /**
   * Atualiza o agendamento de uma conexão
   */
  async updateConnectionSchedule(connection: Connection): Promise<void> {
    if (connection.scheduleEnabled && connection.scheduleFrequency) {
      await this.scheduleConnection(connection)
    } else {
      this.unscheduleConnection(connection.id)
    }
  }

  /**
   * Executa o job de backup para uma conexão
   */
  private async executeBackupJob(connectionId: number): Promise<void> {
    try {
      logger.info(`Executando backup agendado para conexão ${connectionId}`)

      const connection = await Connection.find(connectionId)

      if (!connection) {
        logger.error(`Conexão ${connectionId} não encontrada`)
        this.unscheduleConnection(connectionId)
        return
      }

      if (connection.status !== 'active') {
        logger.warn(
          `Conexão ${connectionId} não está ativa. Pulando backup.`
        )
        return
      }

      if (!connection.scheduleEnabled) {
        logger.warn(
          `Agendamento da conexão ${connectionId} foi desabilitado. Removendo job.`
        )
        this.unscheduleConnection(connectionId)
        return
      }

      const { backup, result } = await this.backupService.execute(
        connection,
        'scheduled'
      )

      if (result.success) {
        logger.info(
          `Backup agendado concluído para conexão ${connectionId}: ${result.fileName} (${backup.getFormattedSize()})`
        )
      } else {
        logger.error(
          `Falha no backup agendado da conexão ${connectionId}: ${result.error}`
        )
      }
    } catch (error) {
      logger.error(
        `Erro ao executar backup agendado da conexão ${connectionId}:`,
        error
      )
    }
  }

  /**
   * Agenda o job de limpeza de backups (retenção)
   */
  private scheduleRetentionJob(): void {
    // Executa diariamente às 2h da manhã
    this.retentionTask = cron.schedule(
      '0 2 * * *',
      async () => {
        await this.executeRetentionJob()
      },
      {
        timezone: 'America/Sao_Paulo',
      }
    )

    logger.info('Job de retenção agendado para executar diariamente às 2h')
  }

  /**
   * Executa o job de limpeza/retenção
   */
  private async executeRetentionJob(): Promise<void> {
    try {
      logger.info('Executando job de retenção de backups...')

      const result = await this.retentionService.pruneBackups()

      logger.info(
        `Job de retenção concluído: ${result.deleted} backups deletados, ${result.promoted} promovidos, ${result.protected} protegidos`
      )

      if (result.errors.length > 0) {
        logger.warn('Erros durante job de retenção:', result.errors)
      }
    } catch (error) {
      logger.error('Erro ao executar job de retenção:', error)
    }
  }

  /**
   * Converte frequência para expressão cron
   */
  private getCronExpression(frequency: ScheduleFrequency): string {
    const expressions: Record<ScheduleFrequency, string> = {
      '1h': '0 * * * *', // A cada hora (minuto 0)
      '6h': '0 */6 * * *', // A cada 6 horas
      '12h': '0 */12 * * *', // A cada 12 horas
      '24h': '0 0 * * *', // Diariamente à meia-noite
    }

    return expressions[frequency]
  }

  /**
   * Retorna estatísticas do scheduler
   */
  getStats(): {
    isRunning: boolean
    activeJobs: number
    connections: Array<{
      connectionId: number
      frequency: ScheduleFrequency
    }>
  } {
    return {
      isRunning: this.isRunning,
      activeJobs: this.jobs.size,
      connections: Array.from(this.jobs.values()).map((job) => ({
        connectionId: job.connectionId,
        frequency: job.frequency,
      })),
    }
  }

  /**
   * Executa manualmente o job de retenção
   */
  async runRetentionNow(): Promise<{
    deleted: number
    promoted: number
    protected: number
    errors: string[]
  }> {
    logger.info('Executando job de retenção manualmente...')
    return await this.retentionService.pruneBackups()
  }
}

// Instância singleton do scheduler
let schedulerInstance: SchedulerService | null = null

/**
 * Obtém a instância singleton do scheduler
 */
export function getScheduler(): SchedulerService {
  if (!schedulerInstance) {
    schedulerInstance = new SchedulerService()
  }
  return schedulerInstance
}
