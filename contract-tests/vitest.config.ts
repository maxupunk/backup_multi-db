import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['tests/**/*.contract.test.ts'],
    globalSetup: ['./src/global-setup.ts'],
    // A suite inteira compartilha UM servidor e UM banco. Rodar arquivos em
    // paralelo tornaria o estado imprevisivel — o teste que apaga uma conexao
    // correria contra o que a lista — e ainda multiplicaria a pressao sobre o
    // rate limiter, que e' por IP. Determinismo vale mais que o tempo de
    // parede aqui.
    fileParallelism: false,
    pool: 'forks',
    poolOptions: { forks: { singleFork: true } },
    testTimeout: 30_000,
    hookTimeout: 180_000,
    teardownTimeout: 60_000,
    reporters: ['default'],
    // Sem retry: um teste de contrato que so' passa na segunda tentativa esta'
    // escondendo instabilidade que o proximo dev vai herdar.
    retry: 0,
  },
})
