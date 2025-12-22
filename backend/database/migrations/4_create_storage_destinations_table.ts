import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'storage_destinations'

  async up() {
    this.schema.createTable(this.tableName, (table) => {
      table.increments('id').primary()

      table.string('name', 100).notNullable().comment('Nome amigável do destino')
      table
        .enum('type', ['local', 's3', 'gcs', 'azure_blob', 'sftp'])
        .notNullable()
        .comment('Tipo do destino de armazenamento')

      table
        .enum('status', ['active', 'inactive'])
        .defaultTo('active')
        .notNullable()
        .comment('Status do destino')

      table
        .boolean('is_default')
        .defaultTo(false)
        .notNullable()
        .comment('Se é o destino padrão para novos backups')

      table
        .text('config_encrypted')
        .notNullable()
        .comment('Configuração criptografada (AES-256-GCM) em JSON')

      table.timestamp('created_at').notNullable()
      table.timestamp('updated_at').notNullable()

      table.index(['type'], 'idx_storage_destinations_type')
      table.index(['status'], 'idx_storage_destinations_status')
      table.index(['is_default'], 'idx_storage_destinations_is_default')
    })
  }

  async down() {
    this.schema.dropTable(this.tableName)
  }
}
