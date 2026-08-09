#!/usr/bin/env node
/**
 * Lancador da suite de contrato (tarefa 1.9 do roadmap).
 *
 * Existe para que os scripts do `package.json` nao precisem de `VAR=valor`
 * antes do comando: isso quebra no `cmd.exe`, e o repositorio e' desenvolvido
 * no Windows. Aqui as variaveis sao montadas em JavaScript e passadas ao
 * processo filho, o que funciona igual nos tres sistemas.
 */

import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)))

const args = process.argv.slice(2)

function flagValue(name, fallback = undefined) {
  const index = args.indexOf(name)
  if (index === -1) return fallback
  const value = args[index + 1]
  if (value === undefined || value.startsWith('--')) {
    console.error(`A flag ${name} precisa de um valor.`)
    process.exit(2)
  }
  return value
}

function hasFlag(name) {
  return args.includes(name)
}

const target = flagValue('--target', 'adonis')
if (target !== 'adonis' && target !== 'roco') {
  console.error(`--target precisa ser \`adonis\` ou \`roco\`, recebi \`${target}\`.`)
  process.exit(2)
}

const record = hasFlag('--record')
const diff = hasFlag('--diff')

if (record && target !== 'adonis') {
  // O golden e' a especificacao extraida do Adonis. Grava-lo do back-roco
  // faria a suite comparar a implementacao com ela mesma e aprovar qualquer
  // desvio — o erro mais caro que esta suite poderia cometer.
  console.error('--record so' + ' e permitido com --target adonis: o golden vem do Adonis.')
  process.exit(2)
}

const env = {
  ...process.env,
  CONTRACT_TARGET: target,
  CONTRACT_RUN_ID: flagValue('--run-id', target),
  CONTRACT_GOLDEN: record ? 'record' : (process.env.CONTRACT_GOLDEN ?? 'compare'),
  CONTRACT_DIFF_REPORT: diff ? '1' : (process.env.CONTRACT_DIFF_REPORT ?? '0'),
  CONTRACT_ENFORCE_COVERAGE: hasFlag('--enforce-coverage')
    ? '1'
    : (process.env.CONTRACT_ENFORCE_COVERAGE ?? '0'),
}

const baseUrl = flagValue('--base-url')
if (baseUrl) env.CONTRACT_BASE_URL = baseUrl

// Repassa ao vitest tudo que nao for flag nossa, para permitir
// `pnpm contract:adonis -- -t "nome do teste"`.
const OWN_FLAGS = new Set(['--target', '--run-id', '--base-url'])
const passthrough = []
for (let index = 0; index < args.length; index++) {
  const arg = args[index]
  if (OWN_FLAGS.has(arg)) {
    index++
    continue
  }
  if (arg === '--record' || arg === '--diff' || arg === '--enforce-coverage') continue
  passthrough.push(arg)
}

const vitest = join(ROOT, 'node_modules', 'vitest', 'vitest.mjs')
if (!existsSync(vitest)) {
  console.error(
    `Nao encontrei o vitest em ${vitest}.\nRode \`pnpm install\` dentro de contract-tests/.`
  )
  process.exit(2)
}

const child = spawn(process.execPath, [vitest, 'run', ...passthrough], {
  cwd: ROOT,
  env,
  stdio: 'inherit',
})

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal)
    return
  }
  process.exit(code ?? 1)
})
