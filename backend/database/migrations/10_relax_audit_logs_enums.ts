import { BaseSchema } from '@adonisjs/lucid/schema'

const TABLE = 'audit_logs'
const TEMP_TABLE = 'audit_logs_rebuild'

const COLUMNS = [
  'id',
  'action',
  'entity_type',
  'entity_id',
  'entity_name',
  'description',
  'details',
  'ip_address',
  'user_agent',
  'status',
  'error_message',
  'created_at',
] as const

const ORIGINAL_ACTIONS = [
  'connection.created',
  'connection.updated',
  'connection.deleted',
  'connection.tested',
  'backup.started',
  'backup.completed',
  'backup.failed',
  'backup.deleted',
  'backup.downloaded',
  'backup.imported',
  'settings.updated',
]

/**
 * Troca os enums de `audit_logs` por colunas de texto.
 *
 * As colunas `action` e `entity_type` foram criadas com `table.enum()`, que no
 * SQLite vira um CHECK constraint. O SQLite nao permite alterar nem remover um
 * CHECK — e o `.alter()` do knex tambem nao o remove, porque preserva as
 * constraints da tabela original. A unica saida e' reconstruir a tabela.
 *
 * Como isso vale para QUALQUER acao nova (foi o que barrou
 * `diagnostics.downloaded`), a reconstrucao aproveita para trocar o enum por
 * texto: `audit_logs` e' append-only, tem um unico escritor (`AuditService`) e
 * os valores validos ja sao garantidos em tempo de compilacao pelos tipos
 * `AuditAction` e `AuditEntityType`. O CHECK duplicava uma validacao existente
 * e cobrava um rebuild a cada acao nova.
 */
export default class extends BaseSchema {
  async up() {
    // 1. Tabela nova, sem indices nomeados (os nomes ainda pertencem a antiga).
    this.schema.createTable(TEMP_TABLE, (table) => {
      table.increments('id').primary()

      table.string('action', 64).notNullable().comment('Tipo de ação realizada')
      table.string('entity_type', 32).notNullable().comment('Tipo da entidade afetada')

      table.integer('entity_id').unsigned().nullable().comment('ID da entidade afetada')
      table.string('entity_name', 255).nullable().comment('Nome da entidade no momento da ação')

      table.text('description').notNullable().comment('Descrição legível da ação')
      table.text('details').nullable().comment('Detalhes adicionais em JSON')

      table.string('ip_address', 45).nullable().comment('Endereço IP do cliente')
      table.string('user_agent', 500).nullable().comment('User-Agent do cliente')

      table
        .enum('status', ['success', 'failure', 'warning'])
        .defaultTo('success')
        .notNullable()
        .comment('Resultado da ação')
      table.text('error_message').nullable().comment('Mensagem de erro, se houver')

      table.timestamp('created_at').notNullable()
    })

    // 2. Copia preservando os ids originais.
    this.defer(async (db) => {
      const columnList = COLUMNS.join(', ')
      await db.rawQuery(
        `INSERT INTO ${TEMP_TABLE} (${columnList}) SELECT ${columnList} FROM ${TABLE}`
      )
    })

    // 3. Remove a antiga (leva junto os indices) e assume o nome dela.
    this.schema.dropTable(TABLE)
    this.schema.renameTable(TEMP_TABLE, TABLE)

    // 4. Recria os indices com os nomes originais.
    this.schema.alterTable(TABLE, (table) => {
      table.index(['action'], 'idx_audit_action')
      table.index(['entity_type'], 'idx_audit_entity_type')
      table.index(['entity_id'], 'idx_audit_entity_id')
      table.index(['status'], 'idx_audit_status')
      table.index(['created_at'], 'idx_audit_created')
      table.index(['entity_type', 'entity_id'], 'idx_audit_entity')
    })
  }

  /**
   * Reconstroi a tabela com os enums originais.
   *
   * Registros com acoes criadas depois desta migration sao descartados na volta
   * — mante-los violaria o CHECK que esta sendo restaurado.
   */
  async down() {
    this.schema.createTable(TEMP_TABLE, (table) => {
      table.increments('id').primary()

      table.enum('action', ORIGINAL_ACTIONS).notNullable()
      table.enum('entity_type', ['connection', 'backup', 'settings']).notNullable()

      table.integer('entity_id').unsigned().nullable()
      table.string('entity_name', 255).nullable()

      table.text('description').notNullable()
      table.text('details').nullable()

      table.string('ip_address', 45).nullable()
      table.string('user_agent', 500).nullable()

      table.enum('status', ['success', 'failure', 'warning']).defaultTo('success').notNullable()
      table.text('error_message').nullable()

      table.timestamp('created_at').notNullable()
    })

    this.defer(async (db) => {
      const columnList = COLUMNS.join(', ')
      const placeholders = ORIGINAL_ACTIONS.map(() => '?').join(', ')

      await db.rawQuery(
        `INSERT INTO ${TEMP_TABLE} (${columnList})
         SELECT ${columnList} FROM ${TABLE}
         WHERE action IN (${placeholders})
           AND entity_type IN ('connection', 'backup', 'settings')`,
        ORIGINAL_ACTIONS
      )
    })

    this.schema.dropTable(TABLE)
    this.schema.renameTable(TEMP_TABLE, TABLE)

    this.schema.alterTable(TABLE, (table) => {
      table.index(['action'], 'idx_audit_action')
      table.index(['entity_type'], 'idx_audit_entity_type')
      table.index(['entity_id'], 'idx_audit_entity_id')
      table.index(['status'], 'idx_audit_status')
      table.index(['created_at'], 'idx_audit_created')
      table.index(['entity_type', 'entity_id'], 'idx_audit_entity')
    })
  }
}
