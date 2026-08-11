/**
 * Relatorios de fim de execucao: cobertura de rotas (1.8) e diff (1.9).
 *
 * A cobertura cruza `docs/routes-baseline.txt` com o rastro que o cliente HTTP
 * deixou. E' o unico mecanismo que impede a Fase 2 de "terminar" com rotas
 * esquecidas: 87 pares metodo+rota nao cabem na cabeca de ninguem, e uma
 * planilha manual desatualiza na primeira semana.
 */

import { mkdirSync, writeFileSync, readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { CONTRACT_ROOT, diffFilePath, loadConfig } from './config.ts'
import { readCoverage } from './coverage.ts'
import { baselineRoutes, type BaselineRoute } from './routes.ts'

export interface CoverageSummary {
  total: number
  covered: number
  uncovered: BaselineRoute[]
  /** Chamadas cujo caminho nao casou com nenhuma rota do baseline. */
  unmatched: Array<{ method: string; path: string; status: number }>
  reportPath: string
}

const REPORTS_DIR = join(CONTRACT_ROOT, 'reports')

function percent(part: number, whole: number): string {
  if (whole === 0) return '0.0'
  return ((part / whole) * 100).toFixed(1)
}

export function buildCoverageReport(): CoverageSummary {
  const config = loadConfig()
  const routes = baselineRoutes()
  const entries = readCoverage()

  const hit = new Set<string>()
  const unmatched = new Map<string, { method: string; path: string; status: number }>()

  for (const entry of entries) {
    if (entry.key) {
      hit.add(entry.key)
      continue
    }
    // Uma chamada que nao casa com o baseline e' um sinal, nao lixo: ou o
    // teste digitou a URL errada, ou o baseline esta' desatualizado. As duas
    // hipoteses valem uma linha no relatorio.
    unmatched.set(`${entry.method} ${entry.path}`, {
      method: entry.method,
      path: entry.path,
      status: entry.status,
    })
  }

  const uncovered = routes.filter((route) => !hit.has(route.key))

  const lines: string[] = [
    '# Cobertura de rotas — suite de contrato',
    '',
    `- Alvo: \`${config.target}\``,
    `- Execucao: \`${config.runId}\``,
    `- Rotas no baseline: **${routes.length}**`,
    `- Exercitadas por algum teste: **${routes.length - uncovered.length}** ` +
      `(${percent(routes.length - uncovered.length, routes.length)}%)`,
    `- Sem teste: **${uncovered.length}**`,
    '',
    '> Uma rota conta como coberta quando um teste faz uma chamada que casa com',
    '> o template dela. O seed nao conta — ele nao afirma nada sobre a resposta.',
    '',
  ]

  if (uncovered.length > 0) {
    lines.push('## Rotas sem teste', '', '| Metodo | Rota | Middleware |', '|---|---|---|')
    for (const route of uncovered) {
      lines.push(`| \`${route.method}\` | \`${route.pattern}\` | ${route.middleware.join(', ') || '—'} |`)
    }
    lines.push('')
  } else {
    lines.push('## Rotas sem teste', '', 'Nenhuma. Todas as rotas do baseline foram exercitadas.', '')
  }

  if (unmatched.size > 0) {
    lines.push(
      '## Chamadas fora do baseline',
      '',
      'Nao casaram com nenhum template conhecido — URL errada no teste ou baseline desatualizado.',
      '',
      '| Metodo | Caminho | Status |',
      '|---|---|---|'
    )
    for (const call of unmatched.values()) {
      lines.push(`| \`${call.method}\` | \`${call.path}\` | ${call.status} |`)
    }
    lines.push('')
  }

  mkdirSync(REPORTS_DIR, { recursive: true })
  const reportPath = join(REPORTS_DIR, 'route-coverage.md')
  writeFileSync(reportPath, lines.join('\n'), 'utf8')

  return {
    total: routes.length,
    covered: routes.length - uncovered.length,
    uncovered,
    unmatched: [...unmatched.values()],
    reportPath,
  }
}

export interface DiffEntry {
  name: string
  target: string
  issues: Array<{ kind: string; message: string }>
}

/**
 * Consolida as divergencias de golden em `reports/contract-diff.md`.
 *
 * Serve ao `contract:diff`: rodar a suite contra o backend e sair com a
 * lista do que ainda nao bate, agrupada por tipo, em vez de um dump do vitest.
 *
 * Quando nao ha divergencias (o caso feliz da Fase 12.2), ainda gera o arquivo
 * com uma declaracao positiva, para que o relatorio de paridade exista.
 */
export function buildDiffReport(): { total: number; reportPath: string } | null {
  const config = loadConfig()
  if (!config.emitDiffReport) return null

  const file = diffFilePath(config)
  const entries: DiffEntry[] = existsSync(file)
    ? readFileSync(file, 'utf8')
        .split('\n')
        .filter((line) => line.trim() !== '')
        .map((line) => JSON.parse(line) as DiffEntry)
    : []

  const byKind = new Map<string, DiffEntry[]>()
  let total = 0

  for (const entry of entries) {
    total += entry.issues.length
    for (const issue of entry.issues) {
      const bucket = byKind.get(issue.kind) ?? []
      bucket.push({ ...entry, issues: [issue] })
      byKind.set(issue.kind, bucket)
    }
  }

  const lines: string[] = [
    '# Diff de contrato',
    '',
    `- Alvo: \`${config.target}\``,
    `- Execucao: \`${config.runId}\``,
    `- Divergencias: **${total}** em **${entries.length}** golden(s)`,
    '',
  ]

  if (total === 0) {
    lines.push(
      'Nenhuma divergencia encontrada entre a resposta do backend e os golden files gerados a partir do Adonis.',
      ''
    )
  }

  for (const [kind, items] of [...byKind.entries()].sort()) {
    lines.push(`## ${kind} (${items.length})`, '')
    for (const item of items) {
      lines.push(`- \`${item.name}\` — ${item.issues[0]!.message}`)
    }
    lines.push('')
  }

  mkdirSync(REPORTS_DIR, { recursive: true })
  const reportPath = join(REPORTS_DIR, 'contract-diff.md')
  writeFileSync(reportPath, lines.join('\n'), 'utf8')

  return { total, reportPath }
}
