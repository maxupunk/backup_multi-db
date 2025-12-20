/*
|--------------------------------------------------------------------------
| Rate Limiter Configuration
|--------------------------------------------------------------------------
|
| This file configures the rate limiter for the application.
| Using memory store for simplicity (ideal for self-hosted apps).
|
*/

import { defineConfig, stores } from '@adonisjs/limiter'

const limiterConfig = defineConfig({
  default: 'memory',
  stores: {
    /**
     * Memory store configuration
     * Ideal for self-hosted applications and single-instance deployments
     */
    memory: stores.memory({}),
  },
})

export default limiterConfig

/**
 * Inferring types for the list of named limiters
 */
declare module '@adonisjs/limiter/types' {
  export interface LimitersList extends InferLimiters<typeof limiterConfig> {}
}
