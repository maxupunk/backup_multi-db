import env from '#start/env'
import { defineConfig, drivers } from '@adonisjs/core/encryption'

export default defineConfig({
  default: 'chacha',
  list: {
    chacha: drivers.chacha20({
      id: 'chacha',
      keys: [env.get('APP_KEY')],
    }),
  },
})
