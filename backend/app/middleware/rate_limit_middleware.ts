/*
|--------------------------------------------------------------------------
| Rate Limiting Middleware
|--------------------------------------------------------------------------
|
| This file defines the rate limiting middleware for HTTP requests.
| It uses @adonisjs/limiter to control the rate of requests.
|
*/

import type { HttpContext } from '@adonisjs/core/http'
import type { NextFn } from '@adonisjs/core/types/http'
import limiter from '@adonisjs/limiter/services/main'

/**
 * Rate limiting configuration
 */
const LIMITS = {
  global: { requests: 100, duration: 60 }, // 100 req/min
  strict: { requests: 10, duration: 60 }, // 10 req/min
  backup: { requests: 5, duration: 300 }, // 5 req/5min
} as const

export type LimiterType = keyof typeof LIMITS

/**
 * Rate limiting middleware
 * Protects API endpoints from abuse by limiting the number of requests per IP
 */
export default class RateLimitMiddleware {
  async handle(ctx: HttpContext, next: NextFn, options?: { limiter?: LimiterType }) {
    const limiterType = options?.limiter ?? 'global'
    const config = LIMITS[limiterType]

    const key = `${limiterType}_${ctx.request.ip()}`

    /**
     * Create a limiter instance with the configuration
     */
    const throttler = limiter.use({
      requests: config.requests,
      duration: config.duration,
    })

    /**
     * Attempt to consume a request
     * Throws an exception if the limit is exceeded
     */
    await throttler.consume(key)

    /**
     * Add rate limit headers to response
     */
    const info = await throttler.get(key)
    if (info) {
      ctx.response.header('X-RateLimit-Limit', config.requests.toString())
      ctx.response.header('X-RateLimit-Remaining', Math.max(0, info.remaining).toString())
      ctx.response.header(
        'X-RateLimit-Reset',
        new Date(Date.now() + info.availableIn * 1000).toISOString()
      )
    }

    return next()
  }
}
