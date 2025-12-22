# Sistema de Notificações Unificado (SSE)

O backend agora utiliza Server-Sent Events (SSE) via `@adonisjs/transmit` para enviar notificações em tempo real para o frontend.

## Canais Disponíveis

Os clientes podem se inscrever nos seguintes canais:

- `notifications/global`: Recebe TODAS as notificações.
- `notifications/system`: Eventos de sistema (início, shutdown, erros gerais).
- `notifications/backup`: Eventos de backup (início, fim, falha, progresso).
- `notifications/storage`: Alertas de armazenamento (espaço baixo, erros).
- `notifications/connection`: Status de conexões (testes, mudanças de estado).

## Estrutura da Notificação

Todas as notificações seguem este formato JSON:

```json
{
  "id": "timestamp-random",
  "type": "info" | "success" | "warning" | "error",
  "category": "system" | "backup" | "storage" | "connection" | "auth",
  "title": "Título da Notificação",
  "message": "Mensagem detalhada",
  "timestamp": "ISO 8601 Date",
  "data": {
    // Dados adicionais específicos do evento
    "event": "backup.completed",
    "connectionId": 1,
    ...
  }
}
```

## Como Usar no Backend

Importe o `NotificationService` e use os métodos estáticos:

```typescript
import { NotificationService } from '#services/notification_service'

// Notificar sucesso
NotificationService.backupCompleted(connName, connId, backupId, fileName, size)

// Notificar erro
NotificationService.backupFailed(connName, connId, error)

// Notificar aviso
NotificationService.storageSpaceLow(destName, percent, bytes)
```

## Frontend (Exemplo de Consumo)

Use o `@adonisjs/transmit-client` no frontend:

```javascript
import { Transmit } from '@adonisjs/transmit-client'

const transmit = new Transmit({
  baseUrl: 'http://localhost:3333',
})

// Inscrever no canal global
const subscription = transmit.subscription('notifications/global')
await subscription.create()

subscription.onMessage((data) => {
  console.log('Nova notificação:', data)
  // Exibir toast/alerta
})
```
