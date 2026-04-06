import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'resource_metric_history'

  async up() {
    this.schema.createTable(this.tableName, (table) => {
      table.increments('id').primary()

      table
        .enum('scope', ['system', 'container'])
        .notNullable()
        .comment('Escopo da métrica: sistema geral ou container individual')
      table.string('entity_id', 128).nullable().comment('ID da entidade (containerId)')
      table.string('entity_name', 255).nullable().comment('Nome da entidade (containerName)')

      table
        .decimal('cpu_usage_percent', 5, 2)
        .notNullable()
        .comment('Percentual de uso de CPU (0-100)')
      table
        .decimal('memory_usage_percent', 5, 2)
        .notNullable()
        .comment('Percentual de uso de memória (0-100)')
      table.bigInteger('memory_used_bytes').notNullable().comment('Memória usada em bytes')
      table.bigInteger('memory_total_bytes').notNullable().comment('Memória total/limite em bytes')

      table.timestamp('collected_at').notNullable().comment('Momento de coleta da amostra')
      table.timestamp('created_at').notNullable()
      table.timestamp('updated_at').notNullable()

      table.index(['scope', 'collected_at'], 'idx_resource_metric_history_scope_collected_at')
      table.index(['entity_id', 'collected_at'], 'idx_resource_metric_history_entity_collected_at')
    })
  }

  async down() {
    this.schema.dropTable(this.tableName)
  }
}
