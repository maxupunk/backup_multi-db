import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'backups'

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
        .comment('ID da conexão de origem')

      // Status do backup
      table
        .enum('status', ['pending', 'running', 'completed', 'failed', 'cancelled'])
        .defaultTo('pending')
        .notNullable()
        .comment('Status atual do backup')

      // Informações do arquivo
      table.string('file_path', 500).nullable().comment('Caminho relativo do arquivo de backup')
      table.string('file_name', 255).nullable().comment('Nome do arquivo de backup')
      table.bigInteger('file_size').unsigned().nullable().comment('Tamanho do arquivo em bytes')
      table.string('checksum', 64).nullable().comment('SHA-256 checksum do arquivo')
      table.boolean('compressed').defaultTo(true).comment('Se o backup foi comprimido (gzip)')

      // Tipo de retenção (GFS)
      table
        .enum('retention_type', ['hourly', 'daily', 'weekly', 'monthly', 'yearly'])
        .defaultTo('hourly')
        .notNullable()
        .comment('Categoria de retenção do backup')
      table
        .boolean('protected')
        .defaultTo(false)
        .comment('Se o backup está protegido contra pruning')

      // Metadados de execução
      table.timestamp('started_at').nullable().comment('Momento de início do backup')
      table.timestamp('finished_at').nullable().comment('Momento de término do backup')
      table.integer('duration_seconds').unsigned().nullable().comment('Duração em segundos')

      // Informações de erro
      table.text('error_message').nullable().comment('Mensagem de erro, se houver')
      table.integer('exit_code').nullable().comment('Código de saída do processo')

      // Metadados adicionais
      table.text('metadata').nullable().comment('Metadados em JSON (tabelas, versão do DB, etc)')
      table
        .enum('trigger', ['scheduled', 'manual'])
        .defaultTo('manual')
        .notNullable()
        .comment('Como o backup foi iniciado')

      // Timestamps
      table.timestamp('created_at').notNullable()
      table.timestamp('updated_at').notNullable()

      // Índices para consultas frequentes
      table.index(['connection_id'], 'idx_backups_connection')
      table.index(['status'], 'idx_backups_status')
      table.index(['retention_type'], 'idx_backups_retention')
      table.index(['created_at'], 'idx_backups_created')
      table.index(['connection_id', 'status'], 'idx_backups_connection_status')
      table.index(['retention_type', 'protected'], 'idx_backups_retention_protected')
    })
  }

  async down() {
    this.schema.dropTable(this.tableName)
  }
}
