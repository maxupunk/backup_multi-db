import env from '#start/env'
import app from '@adonisjs/core/services/app'
import { defineConfig, destination, targets } from '@adonisjs/core/logger'
import { resolveLogLevel } from '#services/log_level_resolver'

const loggerConfig = defineConfig({
  default: 'app',

  /**
   * The loggers object can be used to define multiple loggers.
   * By default, we configure only one logger (named "app").
   */
  loggers: {
    app: {
      enabled: true,
      name: env.get('APP_NAME'),
      level: resolveLogLevel(env.get('NODE_ENV'), env.get('LOG_LEVEL')),
      desination: app.inProduction ? destination(1) : undefined,
      transport: app.inProduction
        ? undefined
        : {
            targets: targets().push(targets.pretty()).toArray(),
          },
    },
  },
})

export default loggerConfig

/**
 * Inferring types for the list of loggers you have configured
 * in your application.
 */
declare module '@adonisjs/core/types' {
  export interface LoggersList extends InferLoggers<typeof loggerConfig> {}
}
