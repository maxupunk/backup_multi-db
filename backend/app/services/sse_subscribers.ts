import transmit from '@adonisjs/transmit/services/main'

/**
 * Informa se ha algum cliente ouvindo o canal.
 *
 * O caso mais comum em producao e ninguem com o dashboard aberto. Sem esta
 * checagem, cada ciclo de polling monta o payload completo (objeto aninhado por
 * container) e chama `broadcast` para ninguem — lixo de heap gerado a cada
 * intervalo, 24h por dia.
 *
 * Em caso de duvida (API indisponivel), assume que ha assinantes: perder um
 * evento e' pior do que gastar um payload.
 */
export function hasSubscribers(channel: string): boolean {
  const getSubscribersFor = (
    transmit as unknown as { getSubscribersFor?: (channel: string) => string[] }
  ).getSubscribersFor

  if (typeof getSubscribersFor !== 'function') {
    return true
  }

  try {
    return getSubscribersFor.call(transmit, channel).length > 0
  } catch {
    return true
  }
}
