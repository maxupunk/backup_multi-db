import { defineConfig } from 'vitest/config'

/**
 * Testes do proprio harness — sem servidor, sem banco, sem rede.
 *
 * Os matchers e a redacao sao o instrumento de medida da Fase 2. Um matcher
 * frouxo demais aprova qualquer coisa e a suite inteira vira teatro; um
 * severo demais reprova o que esta' certo. Por isso o instrumento tem teste
 * proprio, e ele roda sem depender de nada externo.
 */
export default defineConfig({
  test: {
    include: ['tests/selftest/**/*.test.ts'],
    testTimeout: 10_000,
  },
})
