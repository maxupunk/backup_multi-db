import { DateTime } from 'luxon'
import db from '@adonisjs/lucid/services/db'
import logger from '@adonisjs/core/services/logger'
import env from '#start/env'

/**
 * Janela padrao de retencao da auditoria, em dias.
 *
 * O log de auditoria aqui serve para diagnostico rapido ("o que aconteceu com
 * esse backup na semana passada?"), nao como registro historico de longo prazo.
 * A faixa util definida para o produto e de 15 a 30 dias; 30 e o teto dela.
 */
export const DEFAULT_AUDIT_RETENTION_DAYS = 30
export const AUDIT_LOGS_TABLE = 'audit_logs'

/** Teto por execucao: evita um DELETE gigante na primeira poda de uma base antiga. */
const MAX_DELETIONS_PER_RUN = 20_000

/**
 * Limite conservador de parametros por statement no SQLite
 * (SQLITE_MAX_VARIABLE_NUMBER). Um `whereIn` com dezenas de milhares de ids
 * estouraria o limite do driver.
 */
const ID_BATCH_SIZE = 500

export type AuditRetentionResult = {
  deleted: number
  retentionDays: number
  thresholdIso: string
  /** `true` quando o teto por execucao foi atingido e sobrou trabalho. */
  truncated: boolean
}

/**
 * Poda de logs de auditoria.
 *
 * A tabela `audit_logs` so crescia — junto com ela crescem o arquivo SQLite, o
 * custo de qualquer query sobre ela e o tempo de checkpoint do WAL.
 *
 * A janela e curta de proposito (ver DEFAULT_AUDIT_RETENTION_DAYS): o objetivo
 * e diagnostico recente, nao arquivo historico. Ajustavel por
 * `AUDIT_RETENTION_DAYS`; `0` desliga a poda e faz a tabela crescer sem limite.
 */
export class AuditRetentionService {
  static resolveRetentionDays(rawValue = env.get('AUDIT_RETENTION_DAYS')): number {
    if (rawValue === undefined || rawValue === null) {
      return DEFAULT_AUDIT_RETENTION_DAYS
    }

    const parsed = Number(rawValue)

    if (!Number.isFinite(parsed) || parsed < 0) {
      return DEFAULT_AUDIT_RETENTION_DAYS
    }

    return Math.trunc(parsed)
  }

  /**
   * Remove logs mais antigos que a janela de retencao.
   * Retorna 0 exclusoes quando a retencao esta desligada (`0`).
   */
  static async prune(now = DateTime.now()): Promise<AuditRetentionResult> {
    const retentionDays = this.resolveRetentionDays()
    const threshold = now.minus({ days: retentionDays })
    const thresholdSql = threshold.toFormat('yyyy-LL-dd HH:mm:ss')

    if (retentionDays === 0) {
      return {
        deleted: 0,
        retentionDays,
        thresholdIso: threshold.toISO() ?? '',
        truncated: false,
      }
    }

    // Seleciona os ids primeiro para respeitar o teto por execucao — o SQLite
    // nao aceita LIMIT em DELETE sem a compilacao opcional SQLITE_ENABLE_UPDATE_DELETE_LIMIT.
    const rows = (await db
      .from(AUDIT_LOGS_TABLE)
      .select('id')
      .where('created_at', '<', thresholdSql)
      .orderBy('created_at', 'asc')
      .limit(MAX_DELETIONS_PER_RUN)) as Array<{ id: number }>

    if (!rows.length) {
      return {
        deleted: 0,
        retentionDays,
        thresholdIso: threshold.toISO() ?? '',
        truncated: false,
      }
    }

    const ids = rows.map((row) => row.id)
    let deleted = 0

    for (let index = 0; index < ids.length; index += ID_BATCH_SIZE) {
      const batch = ids.slice(index, index + ID_BATCH_SIZE)
      const affected = (await db.from(AUDIT_LOGS_TABLE).whereIn('id', batch).delete()) as unknown

      deleted += typeof affected === 'number' ? affected : batch.length
    }

    const truncated = ids.length === MAX_DELETIONS_PER_RUN

    // Poda de auditoria nunca deve ser silenciosa: fica registrado o que saiu.
    logger.info(
      { deleted, retentionDays, threshold: thresholdSql, truncated },
      `[AuditRetention] ${deleted} registro(s) de auditoria removido(s) (anteriores a ${thresholdSql})`
    )

    return {
      deleted,
      retentionDays,
      thresholdIso: threshold.toISO() ?? '',
      truncated,
    }
  }
}
