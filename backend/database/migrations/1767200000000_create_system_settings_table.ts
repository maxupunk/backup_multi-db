import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'system_settings'

  async up() {
    this.schema.createTable(this.tableName, (table) => {
      table.increments('id').primary()

      table.string('name', 100).notNullable().unique().comment('Nome único da configuração')
      table.text('value').notNullable().comment('Valor serializado em JSON')

      table.timestamp('created_at').notNullable()
      table.timestamp('updated_at').notNullable()

      table.index(['name'], 'idx_system_settings_name')
    })
  }

  async down() {
    this.schema.dropTable(this.tableName)
  }
}
