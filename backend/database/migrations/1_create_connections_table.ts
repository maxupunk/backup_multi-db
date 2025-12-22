import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'connections'

  async up() {
    this.schema.createTable(this.tableName, (table) => {
      // Identificador único
      table.increments('id').primary()

      // Informações básicas da conexão
      table.string('name', 100).notNullable().comment('Nome amigável da conexão')
      table
        .enum('type', ['mysql', 'mariadb', 'postgresql'])
        .notNullable()
        .comment('Tipo do banco de dados')

      // Credenciais de conexão
      table.string('host', 255).notNullable().comment('Host ou IP do servidor')
      table.integer('port').unsigned().notNullable().comment('Porta de conexão')
      table.string('database', 100).notNullable().comment('Nome do banco de dados')
      table.string('username', 100).notNullable().comment('Usuário de conexão')

      // Senha criptografada (AES-256-GCM)
      // Armazena: iv:authTag:encryptedData (base64)
      table.text('password_encrypted').notNullable().comment('Senha criptografada com AES-256-GCM')

      // Configurações de agendamento
      table
        .enum('schedule_frequency', ['1h', '6h', '12h', '24h'])
        .nullable()
        .comment('Frequência de backup agendado')
      table.boolean('schedule_enabled').defaultTo(false).comment('Se o agendamento está ativo')

      // Status e metadados
      table
        .enum('status', ['active', 'inactive', 'error'])
        .defaultTo('active')
        .comment('Status da conexão')
      table.text('last_error').nullable().comment('Último erro de conexão')
      table.timestamp('last_tested_at').nullable().comment('Última vez que a conexão foi testada')
      table.timestamp('last_backup_at').nullable().comment('Última vez que um backup foi realizado')

      // Opções adicionais (JSON serializado)
      table.text('options').nullable().comment('Opções adicionais em JSON (ssl, charset, etc)')

      // Timestamps
      table.timestamp('created_at').notNullable()
      table.timestamp('updated_at').notNullable()

      // Índices
      table.index(['type'], 'idx_connections_type')
      table.index(['status'], 'idx_connections_status')
      table.index(['schedule_enabled'], 'idx_connections_schedule')
    })
  }

  async down() {
    this.schema.dropTable(this.tableName)
  }
}
