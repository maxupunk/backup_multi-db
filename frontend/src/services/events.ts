/**
 * Fluxo de eventos em tempo real (SSE).
 *
 * Uma conexão só para o aplicativo inteiro. O backend recebe a lista de canais
 * na query e devolve cada evento com o nome do canal no campo `event:`, que é
 * o que o `EventSource` usa para despachar por `addEventListener(canal, …)`.
 *
 * ## Por que uma conexão, e não uma por tela
 *
 * O navegador limita ~6 conexões simultâneas por origem em HTTP/1.1. Com uma
 * conexão por componente — notificações, progresso de restauração, métricas do
 * sistema, containers, diagnóstico — o aplicativo encostaria nesse teto e as
 * requisições normais ficariam na fila atrás de fluxos que nunca terminam.
 *
 * Como os canais viajam na URL, mudar o conjunto exige reabrir. A reabertura é
 * adiada até o fim do tick para que várias telas montando juntas produzam uma
 * reconexão só, e não uma por componente.
 */

type Handler = (payload: unknown) => void

const ENDPOINT = '/api/events'

const handlers = new Map<string, Set<Handler>>()

let source: EventSource | null = null
let pendingReopen: ReturnType<typeof setTimeout> | null = null

/**
 * Passa a acompanhar um canal. Devolve a função que cancela a inscrição.
 *
 * O retorno é a única forma de cancelar de propósito: guardar o handler para
 * comparar depois convida a vazamentos quando o componente é desmontado com
 * uma closure diferente da que registrou.
 */
export function subscribe (channel: string, handler: Handler): () => void {
  const existing = handlers.get(channel)

  if (existing) {
    existing.add(handler)
  } else {
    handlers.set(channel, new Set([handler]))
    // Canal novo: a URL mudou, então a conexão precisa ser refeita.
    scheduleReopen()
  }

  return () => {
    const current = handlers.get(channel)
    if (!current) return

    current.delete(handler)
    if (current.size === 0) {
      handlers.delete(channel)
      scheduleReopen()
    }
  }
}

/** Fecha o fluxo. Usado no logout, para não seguir recebendo de outra sessão. */
export function close (): void {
  if (pendingReopen !== null) {
    clearTimeout(pendingReopen)
    pendingReopen = null
  }
  source?.close()
  source = null
}

function scheduleReopen (): void {
  if (pendingReopen !== null) return

  pendingReopen = setTimeout(() => {
    pendingReopen = null
    reopen()
  }, 0)
}

function reopen (): void {
  source?.close()
  source = null

  const channels = [...handlers.keys()]
  if (channels.length === 0) return

  const url = `${ENDPOINT}?channels=${channels.map(encodeURIComponent).join(',')}`
  const stream = new EventSource(url)

  for (const channel of channels) {
    stream.addEventListener(channel, (event) => {
      dispatch(channel, (event as MessageEvent<string>).data)
    })
  }

  // O `EventSource` reconecta sozinho quando a rede cai; só registramos, para
  // que uma queda permanente apareça no console em vez de sumir.
  stream.addEventListener('error', () => {
    console.warn('[events] fluxo SSE interrompido; o navegador vai reconectar')
  })

  source = stream
}

function dispatch (channel: string, raw: string): void {
  let payload: unknown
  try {
    payload = JSON.parse(raw)
  } catch {
    console.warn(`[events] payload ilegível em ${channel}`)
    return
  }

  // Cópia do conjunto: um handler que cancela a própria inscrição durante o
  // despacho mutaria o `Set` que está sendo iterado.
  for (const handler of [...(handlers.get(channel) ?? [])]) {
    handler(payload)
  }
}
