import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  async up() {
    this.schema.alterTable('connections', (table) => {
      table
        .integer('storage_destination_id')
        .unsigned()
        .nullable()
        .references('id')
        .inTable('storage_destinations')
        .onDelete('SET NULL')
        .comment('Destino de armazenamento preferencial da conexão')

      table.index(['storage_destination_id'], 'idx_connections_storage_destination')
    })

    this.schema.alterTable('backups', (table) => {
      table
        .integer('storage_destination_id')
        .unsigned()
        .nullable()
        .references('id')
        .inTable('storage_destinations')
        .onDelete('SET NULL')
        .comment('Destino onde o arquivo de backup foi persistido')

      table.index(['storage_destination_id'], 'idx_backups_storage_destination')
    })
  }

  async down() {
    this.schema.alterTable('backups', (table) => {
      table.dropIndex(['storage_destination_id'], 'idx_backups_storage_destination')
      table.dropColumn('storage_destination_id')
    })

    this.schema.alterTable('connections', (table) => {
      table.dropIndex(['storage_destination_id'], 'idx_connections_storage_destination')
      table.dropColumn('storage_destination_id')
    })
  }
}
