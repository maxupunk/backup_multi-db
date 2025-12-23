import vine from '@vinejs/vine'

/**
 * Tipos de banco de dados suportados
 */
const databaseTypes = ['mysql', 'mariadb', 'postgresql'] as const

/**
 * Frequências de agendamento disponíveis
 */
const scheduleFrequencies = ['1h', '6h', '12h', '24h'] as const

/**
 * Validator para criação de conexão (com múltiplos databases)
 */
export const createConnectionValidator = vine.compile(
  vine.object({
    name: vine.string().trim().minLength(1).maxLength(100),
    type: vine.enum(databaseTypes),
    host: vine.string().trim().minLength(1).maxLength(255),
    port: vine.number().positive().max(65535),
    // Agora aceita array de databases
    databases: vine.array(vine.string().trim().minLength(1).maxLength(100)).minLength(1),
    username: vine.string().trim().minLength(1).maxLength(100),
    password: vine.string().optional(),
    storageDestinationId: vine.number().positive().nullable().optional(),
    scheduleFrequency: vine.enum(scheduleFrequencies).optional(),
    scheduleEnabled: vine.boolean().optional(),
    options: vine
      .object({
        ssl: vine.boolean().optional(),
        charset: vine.string().optional(),
      })
      .optional(),
  })
)

/**
 * Validator para atualização de conexão
 */
export const updateConnectionValidator = vine.compile(
  vine.object({
    name: vine.string().trim().minLength(1).maxLength(100).optional(),
    type: vine.enum(databaseTypes).optional(),
    host: vine.string().trim().minLength(1).maxLength(255).optional(),
    port: vine.number().positive().max(65535).optional(),
    // Permite atualizar a lista de databases
    databases: vine.array(vine.string().trim().minLength(1).maxLength(100)).minLength(1).optional(),
    username: vine.string().trim().minLength(1).maxLength(100).optional(),
    password: vine.string().optional(),
    storageDestinationId: vine.number().positive().nullable().optional(),
    scheduleFrequency: vine.enum(scheduleFrequencies).nullable().optional(),
    scheduleEnabled: vine.boolean().optional(),
    options: vine
      .object({
        ssl: vine.boolean().optional(),
        charset: vine.string().optional(),
      })
      .nullable()
      .optional(),
  })
)

/**
 * Validator para parâmetros de listagem
 */
export const listConnectionsValidator = vine.compile(
  vine.object({
    page: vine.number().positive().optional(),
    limit: vine.number().positive().max(100).optional(),
    type: vine.enum(databaseTypes).optional(),
    status: vine.enum(['active', 'inactive', 'error'] as const).optional(),
    search: vine.string().trim().optional(),
  })
)

/**
 * Validator para descoberta de bancos de dados disponíveis
 */
export const discoverDatabasesValidator = vine.compile(
  vine.object({
    type: vine.enum(databaseTypes),
    host: vine.string().trim().minLength(1).maxLength(255),
    port: vine.number().positive().max(65535),
    username: vine.string().trim().minLength(1).maxLength(100),
    password: vine.string().optional(),
  })
)
