/**
 * Configura o Transmit (Server-Sent Events) e registra as rotas SSE.
 * Também envia notificação de sistema iniciado.
 */

import transmit from '@adonisjs/transmit/services/main'
import logger from '@adonisjs/core/services/logger'
import app from '@adonisjs/core/services/app'
import { NotificationService } from '#services/notification_service'

// Registra as rotas do Transmit para conexões SSE
transmit.registerRoutes()

logger.info('[Transmit] Rotas SSE registradas')

// Aguarda o app estar ready para enviar a notificação de sistema iniciado
app.ready(() => {
  // Pequeno delay para garantir que o Transmit está pronto
  setTimeout(() => {
    NotificationService.systemStarted()
  }, 1000)
})

// Configura hook de shutdown para notificar antes de encerrar
app.terminating(() => {
  NotificationService.systemShutdown()
})
