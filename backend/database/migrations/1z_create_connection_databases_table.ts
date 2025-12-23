import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'connection_databases'

  async up() {
    this.schema.createTable(this.tableName, (table) => {
      // Identificador único
      table.increments('id').primary()

      // Relacionamento com a conexão
      table
        .integer('connection_id')
        .unsigned()
        .notNullable()
        .references('id')
        .inTable('connections')
        .onDelete('CASCADE')
        .comment('ID da conexão pai')

      // Nome do banco de dados
      table
        .string('database_name', 100)
        .notNullable()
        .comment('Nome do banco de dados')

      // Se está habilitado para backup
      table
        .boolean('enabled')
        .defaultTo(true)
        .comment('Se o database está habilitado para backup')

      // Timestamps
      table.timestamp('created_at').notNullable()
      table.timestamp('updated_at').notNullable()

      // Índices
      table.index(['connection_id'], 'idx_conn_db_connection')
      table.index(['connection_id', 'enabled'], 'idx_conn_db_connection_enabled')

      // Constraint única: não pode ter o mesmo database duas vezes na mesma conexão
      table.unique(['connection_id', 'database_name'], {
        indexName: 'idx_conn_db_unique',
      })
    })
  }

  async down() {
    this.schema.dropTable(this.tableName)
  }
}
