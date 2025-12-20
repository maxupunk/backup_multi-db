import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'audit_logs'

  async up() {
    this.schema.createTable(this.tableName, (table) => {
      // Identificador único
      table.increments('id').primary()

      // Tipo de ação realizada
      table
        .enum('action', [
          'connection.created',
          'connection.updated',
          'connection.deleted',
          'connection.tested',
          'backup.started',
          'backup.completed',
          'backup.failed',
          'backup.deleted',
          'backup.downloaded',
          'settings.updated',
        ])
        .notNullable()
        .comment('Tipo de ação realizada')

      // Entidade afetada
      table
        .enum('entity_type', ['connection', 'backup', 'settings'])
        .notNullable()
        .comment('Tipo da entidade afetada')

      table.integer('entity_id').unsigned().nullable().comment('ID da entidade afetada')
      table.string('entity_name', 255).nullable().comment('Nome da entidade no momento da ação')

      // Detalhes da ação
      table.text('description').notNullable().comment('Descrição legível da ação')
      table.text('details').nullable().comment('Detalhes adicionais em JSON')

      // Informações de contexto
      table.string('ip_address', 45).nullable().comment('Endereço IP do cliente')
      table.string('user_agent', 500).nullable().comment('User-Agent do cliente')

      // Resultado da ação
      table
        .enum('status', ['success', 'failure', 'warning'])
        .defaultTo('success')
        .notNullable()
        .comment('Resultado da ação')
      table.text('error_message').nullable().comment('Mensagem de erro, se houver')

      // Timestamps
      table.timestamp('created_at').notNullable()

      // Índices para consultas frequentes
      table.index(['action'], 'idx_audit_action')
      table.index(['entity_type'], 'idx_audit_entity_type')
      table.index(['entity_id'], 'idx_audit_entity_id')
      table.index(['status'], 'idx_audit_status')
      table.index(['created_at'], 'idx_audit_created')
      table.index(['entity_type', 'entity_id'], 'idx_audit_entity')
    })
  }

  async down() {
    this.schema.dropTable(this.tableName)
  }
}
