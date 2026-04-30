import cron from 'node-cron'
import Connection, { type ScheduleFrequency } from '#models/connection'
import { BackupService } from '#services/backup_service'
import {
  BackupRetentionPolicyService,
  DEFAULT_RETENTION_PRUNE_CRON,
} from '#services/backup_retention_policy_service'
import { RetentionService, type RetentionExecutionResult } from '#services/retention_service'
import logger from '@adonisjs/core/services/logger'
import env from '#start/env'

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
  private retentionPolicyService: BackupRetentionPolicyService
  private isRunning = false
  private timezone = env.get('TZ', 'America/Sao_Paulo')

  constructor() {
    this.backupService = new BackupService()
    this.retentionPolicyService = new BackupRetentionPolicyService()
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
    await this.scheduleRetentionJob()

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
      logger.warn(`Tentativa de agendar conexão ${connection.id} sem agendamento ativo`)
      return
    }

    if (connection.status !== 'active') {
      logger.warn(`Tentativa de agendar conexão ${connection.id} sem status ativo`)
      this.unscheduleConnection(connection.id)
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
        timezone: this.timezone,
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
    if (
      connection.scheduleEnabled &&
      connection.scheduleFrequency &&
      connection.status === 'active'
    ) {
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
        logger.warn(`Conexão ${connectionId} não está ativa. Pulando backup.`)
        return
      }

      if (!connection.scheduleEnabled) {
        logger.warn(`Agendamento da conexão ${connectionId} foi desabilitado. Removendo job.`)
        this.unscheduleConnection(connectionId)
        return
      }

      const result = await this.backupService.executeAll(connection, 'scheduled')

      if (result.totalDatabases === 0) {
        logger.warn(`Conexão ${connectionId} não possui databases habilitados para backup agendado`)
        return
      }

      if (result.failed === 0) {
        logger.info(
          `Backup agendado concluído para conexão ${connectionId}: ` +
            `${result.successful}/${result.totalDatabases} database(s) com sucesso`
        )
      } else {
        logger.error(
          `Backup agendado concluído com falhas para conexão ${connectionId}: ` +
            `${result.successful} sucesso, ${result.failed} falha(s)`
        )
      }
    } catch (error) {
      logger.error(`Erro ao executar backup agendado da conexão ${connectionId}:`, error)
    }
  }

  /**
   * Agenda o job de limpeza de backups (retenção)
   */
  private async scheduleRetentionJob(): Promise<void> {
    const policy = await this.retentionPolicyService.getPolicy()
    const cronExpression = this.resolveRetentionCron(policy.pruneCron)

    if (cronExpression !== policy.pruneCron) {
      logger.warn(
        `Expressão de prune inválida: "${policy.pruneCron}". ` +
          `Usando padrão "${DEFAULT_RETENTION_PRUNE_CRON}".`
      )
    }

    this.retentionTask = cron.schedule(
      cronExpression,
      async () => {
        await this.executeRetentionJob()
      },
      {
        timezone: this.timezone,
      }
    )

    logger.info(`Job de retenção agendado com cron "${cronExpression}"`)
  }

  /**
   * Executa o job de limpeza/retenção
   */
  private async executeRetentionJob(): Promise<void> {
    try {
      logger.info('Executando job de retenção de backups...')

      const retentionService = await this.createRetentionService()
      const result = await retentionService.pruneBackups()

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

  async refreshRetentionJob(): Promise<void> {
    if (this.retentionTask) {
      this.retentionTask.stop()
      this.retentionTask = null
    }

    if (!this.isRunning) {
      return
    }

    await this.scheduleRetentionJob()
  }

  private async createRetentionService(): Promise<RetentionService> {
    const policy = await this.retentionPolicyService.getPolicy()

    return new RetentionService(policy)
  }

  private resolveRetentionCron(expression: string): string {
    return cron.validate(expression) ? expression : DEFAULT_RETENTION_PRUNE_CRON
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
  async runRetentionNow(): Promise<RetentionExecutionResult> {
    logger.info('Executando job de retenção manualmente...')
    const retentionService = await this.createRetentionService()
    return await retentionService.pruneBackups()
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
