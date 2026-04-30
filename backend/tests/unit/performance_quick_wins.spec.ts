import { test } from '@japa/runner'

import loggerConfig from '#config/logger'
import { resolveLogLevel } from '#services/log_level_resolver'
import { RESOURCE_METRICS_POLL_INTERVAL_MS } from '#services/resource_metrics_polling_config'

test.group('Performance quick wins', () => {
  test('resource metrics polling interval remains at 10 seconds', ({ assert }) => {
    assert.equal(RESOURCE_METRICS_POLL_INTERVAL_MS, 10_000)
  })

  test('production log level defaults to info when LOG_LEVEL is unset', ({ assert }) => {
    assert.equal(resolveLogLevel('production'), 'info')
  })

  test('non-production log level defaults to debug when LOG_LEVEL is unset', ({ assert }) => {
    assert.equal(resolveLogLevel('development'), 'debug')
    assert.equal(resolveLogLevel('test'), 'debug')
  })

  test('explicit LOG_LEVEL always wins over defaults', ({ assert }) => {
    assert.equal(resolveLogLevel('production', 'warn'), 'warn')
    assert.equal(resolveLogLevel('development', 'error'), 'error')
  })

  test('production logger avoids worker transport configuration', ({ assert }) => {
    const appLogger = loggerConfig.loggers.app

    if (appLogger.desination) {
      assert.isUndefined(appLogger.transport)
      return
    }

    assert.isDefined(appLogger.transport)
  })
})
