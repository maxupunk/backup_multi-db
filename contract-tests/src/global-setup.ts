/**
 * globalSetup do vitest: sobe o alvo, semeia, e no fim relata.
 *
 * Roda no processo principal, antes de qualquer worker existir. E' por isso
 * que `setBaseUrl` conseguir escrever em `process.env` importa: os workers sao
 * forkados depois daqui e herdam a URL do servidor que acabou de subir.
 */

import { rmSync } from 'node:fs'
import { loadConfig, setBaseUrl, coverageFilePath, diffFilePath } from './config.ts'
import { startServer } from './server.ts'
import { seedAll, writeState } from './seed.ts'
import { buildCoverageReport, buildDiffReport } from './report.ts'

export default async function setup(): Promise<() => Promise<void>> {
  const config = loadConfig()

  // Rastros da execucao anterior com o mesmo runId: apagados aqui e nao no
  // teardown, para que um `--run` interrompido no meio ainda deixe o rastro
  // para inspecao.
  rmSync(coverageFilePath(config), { force: true })
  rmSync(diffFilePath(config), { force: true })

  console.log(`[contract] alvo=${config.target} golden=${config.goldenMode} run=${config.runId}`)

  const server = await startServer(config)
  setBaseUrl(server.baseUrl)
  console.log(`[contract] servidor pronto em ${server.baseUrl}`)

  const state = await seedAll(server.baseUrl)
  writeState(state)
  console.log(
    `[contract] seed ok: admin#${state.users.admin.id} member#${state.users.member.id} ` +
      `inactive#${state.users.inactive.id} conexoes=${Object.values(state.connections).filter(Boolean).length}`
  )

  return async () => {
    await server.stop()

    const coverage = buildCoverageReport()
    console.log(
      `[contract] cobertura: ${coverage.covered}/${coverage.total} rotas — ${coverage.reportPath}`
    )
    if (coverage.unmatched.length > 0) {
      console.warn(
        `[contract] ${coverage.unmatched.length} chamada(s) nao casaram com o baseline de rotas`
      )
    }

    const diff = buildDiffReport()
    if (diff) {
      console.log(`[contract] ${diff.total} divergencia(s) de golden — ${diff.reportPath}`)
    }

    if (config.enforceRouteCoverage && coverage.uncovered.length > 0) {
      // Lancar no teardown reprova a execucao inteira, que e' o comportamento
      // pedido em 1.8: rota sem teste quebra o build.
      throw new Error(
        `${coverage.uncovered.length} rota(s) do baseline sem nenhum teste. ` +
          `Detalhes em ${coverage.reportPath}`
      )
    }
  }
}
