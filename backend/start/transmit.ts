/**
 * Configura o Transmit (Server-Sent Events) e registra as rotas SSE.
 * Também envia notificação de sistema iniciado e inicia o broadcast de recursos.
 *
 * O broadcast de recursos (SystemResourceEmitter) só é iniciado quando o app
 * está rodando em contexto HTTP ('web'). Em ace commands (ex: migration:run),
 * getEnvironment() retorna 'console' e o setInterval não é criado, permitindo
 * que o processo termine normalmente após a execução do comando.
 */


import logger from '@adonisjs/core/services/logger'
import app from '@adonisjs/core/services/app'
import { NotificationService } from '#services/notification_service'
import { SystemResourceEmitter } from '#services/system_resource_emitter'

// Apenas exibe o log (as rotas foram registradas em start/routes.ts)
logger.info('[Transmit] Rotas SSE configuradas')

// Aguarda o app estar ready — mas só age em contexto de servidor HTTP.
// Ace commands (migration:run, etc.) rodam em ambiente 'console' e NÃO devem
// iniciar o setInterval, pois isso impediria o processo de finalizar.
app.ready(() => {
  if (app.getEnvironment() !== 'web') {
    return
  }

  // Pequeno delay para garantir que o Transmit está totalmente pronto
  setTimeout(() => {
    NotificationService.systemStarted()
    SystemResourceEmitter.start()
  }, 1000)
})

// Configura hook de shutdown para notificar antes de encerrar (apenas em web)
app.terminating(() => {
  if (app.getEnvironment() !== 'web') {
    return
  }

  SystemResourceEmitter.stop()
  NotificationService.systemShutdown()
})
