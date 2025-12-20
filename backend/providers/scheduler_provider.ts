import type { ApplicationService } from '@adonisjs/core/types'
import { getScheduler } from '#services/scheduler_service'

/**
 * Provider para iniciar o serviço de agendamento de backups
 */
export default class SchedulerProvider {
  constructor(protected app: ApplicationService) {}

  /**
   * Inicia o scheduler quando a aplicação bootar
   */
  async boot() {
    // Apenas iniciar em produção ou quando NODE_ENV != test
    const env = this.app.nodeEnvironment

    if (env === 'test') {
      return
    }

    this.app.ready(() => {
      const scheduler = getScheduler()
      scheduler.start().catch((error) => {
        console.error('Erro ao iniciar scheduler:', error)
      })
    })
  }

  /**
   * Para o scheduler ao desligar a aplicação
   */
  async shutdown() {
    const scheduler = getScheduler()
    scheduler.stop()
  }
}
