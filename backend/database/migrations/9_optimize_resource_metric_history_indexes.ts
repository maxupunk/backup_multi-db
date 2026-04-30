import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'resource_metric_history'

  async up() {
    this.schema.alterTable(this.tableName, (table) => {
      table.index(
        ['collected_at', 'scope', 'entity_id'],
        'idx_resource_metric_history_collected_scope_entity'
      )
    })
  }

  async down() {
    this.schema.alterTable(this.tableName, (table) => {
      table.dropIndex(
        ['collected_at', 'scope', 'entity_id'],
        'idx_resource_metric_history_collected_scope_entity'
      )
    })
  }
}
