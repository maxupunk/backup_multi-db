export const RESOURCE_METRICS_POLL_INTERVAL_MS = 10_000

/**
 * Intervalo usado quando ninguem esta assinando os canais SSE de metricas —
 * o caso mais comum em producao (dashboard fechado).
 *
 * Nao reduz a granularidade do historico: a persistencia ja tem intervalo
 * minimo de 60s, entao os pontos gravados continuam iguais. Ao chegar um
 * assinante, o servico volta imediatamente ao intervalo ativo e dispara um
 * ciclo na hora.
 */
export const RESOURCE_METRICS_IDLE_POLL_INTERVAL_MS = 30_000
