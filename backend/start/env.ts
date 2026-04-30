/*
|--------------------------------------------------------------------------
| Environment variables service
|--------------------------------------------------------------------------
|
| The `Env.create` method creates an instance of the Env service. The
| service validates the environment variables and also cast values
| to JavaScript data types.
|
*/

import { Env } from '@adonisjs/core/env'
import { LOG_LEVEL_VALUES } from '#services/log_level_resolver'

export default await Env.create(new URL('../../', import.meta.url), {
  NODE_ENV: Env.schema.enum(['development', 'production', 'test'] as const),
  TZ: Env.schema.string.optional(),
  PORT: Env.schema.number(),
  APP_KEY: Env.schema.string(),
  HOST: Env.schema.string({ format: 'host' }),
  LOG_LEVEL: Env.schema.enum.optional(LOG_LEVEL_VALUES),

  /*
  |--------------------------------------------------------------------------
  | Criptografia de senhas dos bancos de dados
  |--------------------------------------------------------------------------
  */
  DB_ENCRYPTION_KEY: Env.schema.string(),

  /*
  |--------------------------------------------------------------------------
  | Configuracoes de autenticacao
  |--------------------------------------------------------------------------
  */
  AUTH_ACCESS_TOKEN_EXPIRES_IN: Env.schema.string.optional(),
  INITIAL_ADMIN_BOOTSTRAP_TOKEN: Env.schema.string.optional(),

  /*
  |--------------------------------------------------------------------------
  | Configurações de backup
  |--------------------------------------------------------------------------
  */
  BACKUP_STORAGE_PATH: Env.schema.string.optional(),
  SQLITE_DATABASE_PATH: Env.schema.string.optional(),

  /*
  |--------------------------------------------------------------------------
  | Configurações de retenção (em unidades respectivas)
  |--------------------------------------------------------------------------
  */
  RETENTION_DAILY: Env.schema.number.optional(),
  RETENTION_WEEKLY: Env.schema.number.optional(),
  RETENTION_MONTHLY: Env.schema.number.optional(),
  RETENTION_YEARLY: Env.schema.number.optional(),
  RETENTION_PRUNE_CRON: Env.schema.string.optional(),
})
