export const LOG_LEVEL_VALUES = [
  'fatal',
  'error',
  'warn',
  'info',
  'debug',
  'trace',
  'silent',
] as const

export type LogLevel = (typeof LOG_LEVEL_VALUES)[number]
export type AppNodeEnv = 'development' | 'production' | 'test'

export function resolveLogLevel(nodeEnv: AppNodeEnv, configuredLogLevel?: LogLevel): LogLevel {
  if (configuredLogLevel) {
    return configuredLogLevel
  }

  return nodeEnv === 'production' ? 'info' : 'debug'
}
