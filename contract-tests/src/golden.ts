/**
 * Golden files (tarefa 1.6 do roadmap).
 *
 * O golden e' gravado a partir do **Adonis** e vira a especificacao
 * executavel do back-roco. Um golden so' presta se satisfizer duas coisas ao
 * mesmo tempo, que puxam em direcoes opostas:
 *
 * 1. ser estavel — gravar duas vezes tem que dar bytes identicos, senao o
 *    `git diff` enche de ruido e ninguem nota a mudanca de verdade;
 * 2. ser preciso — nao pode borrar exatamente o que precisa ser comparado.
 *
 * A saida: o arquivo guarda **duas** representacoes da resposta. `shape` sai
 * do corpo **cru** e e' a autoridade da comparacao; `body` sai do corpo
 * **redigido** e existe so' para leitura humana no code review. Se a redacao
 * fosse a unica representacao, um numero virado em `"<id>"` faria o contrato
 * "id e' number" desaparecer do golden sem que ninguem percebesse.
 */

import { mkdirSync, readFileSync, writeFileSync, appendFileSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { diffFilePath, loadConfig, type Target } from './config.ts'
import { redact, redactHeaders, type RedactOptions } from './redact.ts'
import {
  compareShapes,
  formatIssues,
  shapeOf,
  type CompareOptions,
  type Shape,
  type ShapeIssue,
} from './shape.ts'
import { describeResponse, type ContractResponse } from './http.ts'

export interface GoldenRecord {
  name: string
  /** Implementacao de onde o golden foi gravado. */
  recordedFrom: Target
  request: {
    method: string
    /** Template do baseline, quando a URL casou. */
    route: string | null
    path: string
    /** Papel usado na chamada: `admin`, `user`, `none`. */
    as: string
  }
  response: {
    status: number
    /** Mime sem parametros, ex.: `application/json`. */
    mime: string
    /** Valor completo do content-type, so' para leitura. */
    contentType: string
    /** Nomes de cabecalho nao volateis, ordenados. */
    headerNames: string[]
    headers: Record<string, string>
    /** Derivado do corpo CRU. E' o que a comparacao usa. */
    shape: Shape
    /** Corpo redigido. Leitura humana; nao participa da comparacao. */
    body: unknown
  }
}

export interface GoldenOptions extends RedactOptions {
  /** Papel usado na chamada, para constar no arquivo. */
  as?: string
  /** Tolerancias da comparacao de formato. */
  compare?: CompareOptions
  /**
   * Cabecalhos cujo *valor* tambem entra na comparacao.
   * Por padrao so' o mime e' comparado; nome de cabecalho sempre entra.
   */
  assertHeaders?: string[]
}

export interface GoldenResult {
  mode: 'record' | 'compare' | 'off'
  /** Divergencias encontradas. Vazio quando nao ha' golden a comparar. */
  issues: GoldenIssue[]
  record: GoldenRecord
}

export interface GoldenIssue {
  kind: 'status' | 'mime' | 'header' | 'shape' | 'missing-golden'
  message: string
  expected?: string
  actual?: string
  shapeIssue?: ShapeIssue
}

function goldenPath(name: string): string {
  const config = loadConfig()
  if (!/^[a-z0-9][a-z0-9._/-]*$/.test(name)) {
    throw new Error(
      `Nome de golden invalido: ${JSON.stringify(name)}. Use minusculas, digitos, ` +
        `\`.\`, \`-\`, \`_\` e \`/\` — o nome vira caminho de arquivo.`
    )
  }
  return join(config.goldenDir, `${name}.json`)
}

function mimeOf(contentType: string): string {
  return contentType.split(';')[0]!.trim().toLowerCase()
}

function buildRecord(
  name: string,
  response: ContractResponse,
  options: GoldenOptions
): GoldenRecord {
  const config = loadConfig()
  const headers = redactHeaders(response.headers)

  return {
    name,
    recordedFrom: config.target,
    request: {
      method: response.method,
      route: response.route,
      path: response.path,
      as: options.as ?? 'none',
    },
    response: {
      status: response.status,
      mime: mimeOf(response.contentType),
      contentType: response.contentType,
      headerNames: Object.keys(headers),
      headers,
      // Formato do corpo CRU: a redacao troca tipos (number -> "<id>") e
      // destruiria a informacao que a comparacao precisa.
      shape: shapeOf(response.body),
      // Os caminhos que a comparacao ignora tambem saem do corpo gravado. Sao
      // justamente os que dependem da maquina — uso de CPU, memoria livre,
      // latencia —, e mante-los faria o golden mudar a cada execucao sem que
      // nada de contrato tivesse mudado.
      body: redact(response.body, {
        ...options,
        notComparedPaths: [...(options.notComparedPaths ?? []), ...(options.compare?.ignorePaths ?? [])],
      }),
    },
  }
}

function readGolden(name: string): GoldenRecord | null {
  const file = goldenPath(name)
  if (!existsSync(file)) return null

  try {
    return JSON.parse(readFileSync(file, 'utf8')) as GoldenRecord
  } catch (cause) {
    throw new Error(`Golden corrompido em ${file}`, { cause })
  }
}

function writeGolden(record: GoldenRecord): void {
  const file = goldenPath(record.name)
  mkdirSync(dirname(file), { recursive: true })
  // Sem timestamp de gravacao no arquivo, de proposito: um campo desses faria
  // toda re-gravacao aparecer como alteracao no `git diff`, escondendo a
  // mudanca de contrato no meio do ruido.
  writeFileSync(file, JSON.stringify(record, null, 2) + '\n', 'utf8')
}

function recordDiff(name: string, issues: GoldenIssue[]): void {
  const config = loadConfig()
  if (!config.emitDiffReport || issues.length === 0) return

  try {
    const file = diffFilePath(config)
    mkdirSync(dirname(file), { recursive: true })
    appendFileSync(file, JSON.stringify({ name, target: config.target, issues }) + '\n', 'utf8')
  } catch {
    // Relatorio e' acessorio; a falha do teste ja' foi reportada.
  }
}

function compareRecords(
  actual: GoldenRecord,
  expected: GoldenRecord,
  options: GoldenOptions
): GoldenIssue[] {
  const issues: GoldenIssue[] = []

  if (actual.response.status !== expected.response.status) {
    issues.push({
      kind: 'status',
      expected: String(expected.response.status),
      actual: String(actual.response.status),
      message: `status ${actual.response.status}, golden tem ${expected.response.status}`,
    })
  }

  if (actual.response.mime !== expected.response.mime) {
    // So' o mime, sem `charset`: `application/json` e
    // `application/json; charset=utf-8` sao o mesmo contrato para o cliente, e
    // exigir o parametro so' geraria falso-negativo entre as duas stacks.
    issues.push({
      kind: 'mime',
      expected: expected.response.mime,
      actual: actual.response.mime,
      message: `content-type ${actual.response.mime}, golden tem ${expected.response.mime}`,
    })
  }

  const missingHeaders = expected.response.headerNames.filter(
    (header) => !actual.response.headerNames.includes(header)
  )
  for (const header of missingHeaders) {
    issues.push({
      kind: 'header',
      expected: header,
      actual: 'ausente',
      message: `cabecalho \`${header}\` presente no golden e ausente na resposta`,
    })
  }

  for (const header of options.assertHeaders ?? []) {
    const key = header.toLowerCase()
    const expectedValue = expected.response.headers[key]
    const actualValue = actual.response.headers[key]
    if (expectedValue !== actualValue) {
      issues.push({
        kind: 'header',
        expected: expectedValue ?? 'ausente',
        actual: actualValue ?? 'ausente',
        message: `cabecalho \`${key}\`: ${actualValue ?? 'ausente'}, golden tem ${expectedValue ?? 'ausente'}`,
      })
    }
  }

  for (const shapeIssue of compareShapes(
    actual.response.shape,
    expected.response.shape,
    options.compare
  )) {
    issues.push({ kind: 'shape', message: shapeIssue.message, shapeIssue })
  }

  return issues
}

/**
 * Confronta uma resposta com o golden de mesmo nome.
 *
 * Nao lanca — devolve o resultado. Quem decide se uma divergencia reprova o
 * teste e' `expectGolden`, logo abaixo; separar as duas coisas permite a um
 * teste inspecionar as diferencas antes de julgar.
 */
export function checkGolden(
  name: string,
  response: ContractResponse,
  options: GoldenOptions = {}
): GoldenResult {
  const config = loadConfig()
  const actual = buildRecord(name, response, options)

  if (config.goldenMode === 'off') {
    return { mode: 'off', issues: [], record: actual }
  }

  if (config.goldenMode === 'record') {
    writeGolden(actual)
    return { mode: 'record', issues: [], record: actual }
  }

  const expected = readGolden(name)
  if (!expected) {
    const issue: GoldenIssue = {
      kind: 'missing-golden',
      message:
        `Nao existe golden \`${name}\`. Grave rodando \`pnpm contract:record\` ` +
        `contra o Adonis antes de comparar.`,
    }
    recordDiff(name, [issue])
    return { mode: 'compare', issues: [issue], record: actual }
  }

  const issues = compareRecords(actual, expected, options)
  recordDiff(name, issues)

  return { mode: 'compare', issues, record: actual }
}

/**
 * Versao que reprova o teste na primeira divergencia.
 *
 * A mensagem carrega a resposta inteira: sem ela, um `shape` divergente obriga
 * a rodar tudo de novo com log ligado so' para ver o corpo.
 */
export function expectGolden(
  name: string,
  response: ContractResponse,
  options: GoldenOptions = {}
): GoldenResult {
  const result = checkGolden(name, response, options)

  if (result.issues.length > 0) {
    const lines = result.issues.map((issue) => `  - ${issue.message}`).join('\n')
    throw new Error(
      `Golden \`${name}\` divergiu (${result.issues.length}):\n${lines}\n\n` +
        `Resposta observada:\n${describeResponse(response)}`
    )
  }

  return result
}

export { formatIssues }
