import { defineConfig } from 'vitest/config'
import type { TestSpecification } from 'vitest/node'

/**
 * Ordena os arquivos de teste por nome, sempre.
 *
 * O sequenciador padrão do vitest ordena por tamanho e por duração das
 * execuções anteriores, então a ordem muda entre rodadas. Numa suíte que
 * compartilha **um** banco isso torna o resultado irreprodutível: `GET
 * /api/users` devolve "os usuários criados até agora", e quais são depende de
 * quais arquivos rodaram antes. Os golden files passariam a mudar sozinhos, e
 * um arquivo que muda sozinho para de servir como registro de mudança de
 * contrato.
 */
class AlphabeticalSequencer {
  async shard(files: TestSpecification[]): Promise<TestSpecification[]> {
    return files
  }

  async sort(files: TestSpecification[]): Promise<TestSpecification[]> {
    return [...files].sort((a, b) => a.moduleId.localeCompare(b.moduleId))
  }
}

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
    sequence: { sequencer: AlphabeticalSequencer },
    testTimeout: 30_000,
    hookTimeout: 180_000,
    teardownTimeout: 60_000,
    reporters: ['default'],
    // Sem retry: um teste de contrato que so' passa na segunda tentativa esta'
    // escondendo instabilidade que o proximo dev vai herdar.
    retry: 0,
  },
})
