import vine from '@vinejs/vine'

/**
 * Modos de restauração suportados
 */
const restoreModes = ['full', 'schema-only', 'data-only'] as const

/**
 * Validator para restauração de backup
 */
export const restoreBackupValidator = vine.compile(
  vine.object({
    /** Modo de restauração */
    mode: vine.enum(restoreModes).optional(),
    /** Database de destino (sobrescreve o original) */
    targetDatabase: vine.string().trim().minLength(1).maxLength(100).optional(),
    /** PostgreSQL: Não restaurar owners */
    noOwner: vine.boolean().optional(),
    /** PostgreSQL: Não restaurar privilégios */
    noPrivileges: vine.boolean().optional(),
    /** PostgreSQL: Não restaurar tablespaces */
    noTablespaces: vine.boolean().optional(),
    /** PostgreSQL: Não restaurar comentários */
    noComments: vine.boolean().optional(),
    /** MySQL/MariaDB: Não executar CREATE DATABASE / USE */
    noCreateDb: vine.boolean().optional(),
    /** Pular verificação e backup de segurança antes da restauração */
    skipSafetyBackup: vine.boolean().optional(),
  })
)
