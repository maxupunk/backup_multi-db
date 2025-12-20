import vuetify from 'eslint-config-vuetify'

const config = vuetify()

export default [
  ...Array.from(config),
  {
    ignores: [
      'dist/**',
      'dev-dist/**',
    ],
  },
]
