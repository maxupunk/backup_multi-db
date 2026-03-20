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
    // Apenas iniciar quando a aplicação está rodando como servidor HTTP.
    // 'console' = ace commands (migration:run, etc.), 'test' = testes, 'repl' = repl.
    if (this.app.getEnvironment() !== 'web') {
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
