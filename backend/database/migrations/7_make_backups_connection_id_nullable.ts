import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'backups'

  async up() {
    this.schema.alterTable(this.tableName, (table) => {
      // Permite backups importados sem associação a uma conexão
      table
        .integer('connection_id')
        .unsigned()
        .nullable()
        .references('id')
        .inTable('connections')
        .onDelete('CASCADE')
        .alter()
    })
  }

  async down() {
    this.schema.alterTable(this.tableName, (table) => {
      table
        .integer('connection_id')
        .unsigned()
        .notNullable()
        .references('id')
        .inTable('connections')
        .onDelete('CASCADE')
        .alter()
    })
  }
}
