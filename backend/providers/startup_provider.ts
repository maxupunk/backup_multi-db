import type { ApplicationService } from '@adonisjs/core/types'
import logger from '@adonisjs/core/services/logger'
import { StorageDestinationService } from '#services/storage_destination_service'

export default class StartupProvider {
  constructor(protected app: ApplicationService) {}

  async boot() {
    if (this.app.getEnvironment() !== 'web') {
      return
    }

    this.app.ready(async () => {
      try {
        await StorageDestinationService.ensureDefaultLocalDestination()
      } catch (error) {
        logger.error({ err: error }, '[Startup] Falha ao garantir destino local padrão')
      }
    })
  }
}
