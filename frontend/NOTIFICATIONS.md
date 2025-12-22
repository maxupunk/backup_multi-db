# Sistema de Notificações Frontend

O frontend utiliza `@adonisjs/transmit-client` para receber notificações em tempo real do backend.

## Componentes

- **Store (`stores/notification.ts`)**: Gerencia o estado das notificações (ativas e histórico).
- **Plugin (`plugins/transmit.ts`)**: Inicializa a conexão SSE e escuta os canais globais.
- **Componente (`components/NotificationToast.vue`)**: Exibe as notificações em formato de lista (toasts) no canto superior direito.

## Como Usar

### Exibir Notificação Local (Manual)

Se precisar exibir uma notificação que não vem do backend (ex: erro de validação local):

```typescript
import { useNotificationStore } from "@/stores/notification";

const notificationStore = useNotificationStore();

notificationStore.add({
  type: "success", // 'info' | 'success' | 'warning' | 'error'
  category: "system",
  title: "Sucesso",
  message: "Operação realizada com sucesso",
});
```

Ou usando o `inject` (compatibilidade legada):

```typescript
const showNotification = inject("showNotification");
showNotification("Mensagem de erro", "error");
```

### Canais Automáticos

O plugin `transmit` já se inscreve automaticamente nos seguintes canais:

- `notifications/global`
- `notifications/system`
- `notifications/backup`
- `notifications/storage`
- `notifications/connection`

Qualquer evento enviado pelo backend nestes canais aparecerá automaticamente na tela.

## Configuração

A configuração da URL do backend é feita via Proxy no `vite.config.mts`:

```typescript
'/__transmit': {
  target: 'http://localhost:3333',
  changeOrigin: true,
  secure: false,
}
```

Isso evita problemas de CORS e permite usar `window.location.origin` no cliente.
