/**
 * Matchers tolerantes (tarefa 1.7 do roadmap).
 *
 * A suite compara o *formato* da resposta, nao o valor. Comparar valor
 * literal produziria falso-negativo em toda execucao: id incremental,
 * timestamp, duracao, ordem de chaves. O que precisa continuar igual entre o
 * Adonis e o back-roco e' a estrutura — quais chaves existem e de que tipo
 * sao —, porque e' disso que o frontend depende.
 *
 * As tolerancias sao deliberadas e cada uma esta anotada onde e' aplicada.
 */

export type Shape =
  | { kind: 'primitive'; type: 'string' | 'number' | 'boolean' | 'null' }
  | { kind: 'array'; element: Shape | null }
  | { kind: 'object'; fields: Record<string, Shape> }

export interface ShapeIssue {
  /** Caminho JSON do problema, ex.: `data.user.email`. */
  path: string
  kind: 'missing-key' | 'extra-key' | 'type-mismatch' | 'unverified-array'
  expected: string
  actual: string
  message: string
}

function primitive(type: 'string' | 'number' | 'boolean' | 'null'): Shape {
  return { kind: 'primitive', type }
}

/**
 * Une os formatos de dois elementos de um mesmo array.
 *
 * Arrays heterogeneos aparecem de verdade (ex.: um campo opcional presente em
 * uns itens e ausente em outros). Unir e' mais fiel que olhar so' o item 0:
 * campos ausentes viram opcionais na uniao, e tipos conflitantes viram
 * `unknown`, que casa com qualquer coisa e evita falso-negativo.
 */
function unify(a: Shape, b: Shape): Shape {
  if (a.kind === 'primitive' && b.kind === 'primitive') {
    if (a.type === b.type) return a
    // `null` combinado com um tipo concreto e' um campo nulavel, nao um
    // conflito. Preserva o tipo concreto — que e' o que interessa validar.
    if (a.type === 'null') return b
    if (b.type === 'null') return a
    return primitive('string') // tipos realmente conflitantes: nao ha' o que afirmar
  }

  if (a.kind === 'array' && b.kind === 'array') {
    if (a.element === null) return b
    if (b.element === null) return a
    return { kind: 'array', element: unify(a.element, b.element) }
  }

  if (a.kind === 'object' && b.kind === 'object') {
    const fields: Record<string, Shape> = {}
    for (const key of new Set([...Object.keys(a.fields), ...Object.keys(b.fields)])) {
      const left = a.fields[key]
      const right = b.fields[key]
      if (left && right) fields[key] = unify(left, right)
      // Chave presente em so' um dos itens: mantem, mas como nulavel, para nao
      // exigir de todo item algo que nem o gravado tinha em todo item.
      else fields[key] = unify(left ?? right!, primitive('null'))
    }
    return { kind: 'object', fields }
  }

  return a
}

/** Deriva o formato de um valor JSON ja' desserializado. */
export function shapeOf(value: unknown): Shape {
  if (value === null || value === undefined) return primitive('null')

  if (Array.isArray(value)) {
    if (value.length === 0) return { kind: 'array', element: null }
    return {
      kind: 'array',
      element: value.map(shapeOf).reduce(unify),
    }
  }

  if (typeof value === 'object') {
    const fields: Record<string, Shape> = {}
    // Ordena as chaves: a ordem de serializacao nao faz parte do contrato e
    // nao pode virar diff.
    for (const key of Object.keys(value as Record<string, unknown>).sort()) {
      fields[key] = shapeOf((value as Record<string, unknown>)[key])
    }
    return { kind: 'object', fields }
  }

  if (typeof value === 'string') return primitive('string')
  if (typeof value === 'number') return primitive('number')
  if (typeof value === 'boolean') return primitive('boolean')

  return primitive('string')
}

function describe(shape: Shape): string {
  switch (shape.kind) {
    case 'primitive':
      return shape.type
    case 'array':
      return shape.element === null ? 'array<vazio>' : `array<${describe(shape.element)}>`
    case 'object':
      return `object{${Object.keys(shape.fields).join(',')}}`
  }
}

export interface CompareOptions {
  /**
   * Chaves extras na resposta observada sao um problema?
   *
   * Sao, sim, para a paridade que a Fase 2 exige: um campo a mais no
   * back-roco e' desvio de contrato mesmo que o frontend o ignore hoje. Fica
   * configuravel porque durante a Fase 4-11 o back-roco vai crescer aos
   * poucos e pode ser util afrouxar temporariamente.
   */
  allowExtraKeys?: boolean
  /**
   * Caminhos (com `*` para indice de array) que nao entram na comparacao.
   * Serve para campos legitimamente variaveis em formato, ex.: um `config`
   * cujo conteudo depende do provider.
   */
  ignorePaths?: string[]
}

function pathMatches(path: string, patterns: string[]): boolean {
  return patterns.some((pattern) => {
    const regex = new RegExp(
      '^' +
        pattern
          .split('.')
          .map((part) => (part === '*' ? '[^.]+' : part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))
          .join('\\.') +
        '$'
    )
    return regex.test(path)
  })
}

/**
 * Compara o formato observado com o esperado e devolve todas as divergencias.
 *
 * Devolve lista em vez de lancar: um relatorio com as cinco diferencas de uma
 * resposta vale muito mais que uma excecao na primeira.
 */
export function compareShapes(
  actual: Shape,
  expected: Shape,
  options: CompareOptions = {},
  path = ''
): ShapeIssue[] {
  const { allowExtraKeys = false, ignorePaths = [] } = options

  if (path !== '' && pathMatches(path, ignorePaths)) return []

  const issues: ShapeIssue[] = []
  const at = path === '' ? '(raiz)' : path

  if (expected.kind === 'primitive' && expected.type === 'null') {
    // O golden gravou `null` ali. Qualquer tipo concreto e' compativel: o
    // campo e' nulavel e na gravacao calhou de estar nulo.
    return []
  }

  if (actual.kind === 'primitive' && actual.type === 'null') {
    // Espelho do caso acima: valor nulo onde o golden viu um tipo concreto.
    // Tambem e' nulabilidade, nao quebra de contrato.
    return []
  }

  if (actual.kind !== expected.kind) {
    return [
      {
        path: at,
        kind: 'type-mismatch',
        expected: describe(expected),
        actual: describe(actual),
        message: `${at}: esperava ${describe(expected)}, recebi ${describe(actual)}`,
      },
    ]
  }

  if (actual.kind === 'primitive' && expected.kind === 'primitive') {
    if (actual.type !== expected.type) {
      issues.push({
        path: at,
        kind: 'type-mismatch',
        expected: expected.type,
        actual: actual.type,
        message: `${at}: esperava ${expected.type}, recebi ${actual.type}`,
      })
    }
    return issues
  }

  if (actual.kind === 'array' && expected.kind === 'array') {
    if (expected.element === null) return issues // golden gravou array vazio: nada a afirmar

    if (actual.element === null) {
      // Array vazio onde o golden tinha itens. Nao e' quebra provada, mas
      // tambem nao e' verificacao: o formato do item ficou sem checar. Reporta
      // como `unverified-array` para o teste decidir — silenciar aqui daria a
      // falsa impressao de que o item foi validado.
      issues.push({
        path: at,
        kind: 'unverified-array',
        expected: describe(expected),
        actual: 'array<vazio>',
        message:
          `${at}: array vazio; o formato do item nao pode ser verificado ` +
          `(o golden esperava ${describe(expected.element)}). Faltou seed?`,
      })
      return issues
    }

    return compareShapes(actual.element, expected.element, options, path === '' ? '*' : `${path}.*`)
  }

  if (actual.kind === 'object' && expected.kind === 'object') {
    for (const [key, expectedField] of Object.entries(expected.fields)) {
      const childPath = path === '' ? key : `${path}.${key}`
      const actualField = actual.fields[key]

      if (!actualField) {
        if (pathMatches(childPath, ignorePaths)) continue
        issues.push({
          path: childPath,
          kind: 'missing-key',
          expected: describe(expectedField),
          actual: 'ausente',
          message: `${childPath}: chave ausente na resposta (esperava ${describe(expectedField)})`,
        })
        continue
      }

      issues.push(...compareShapes(actualField, expectedField, options, childPath))
    }

    if (!allowExtraKeys) {
      for (const key of Object.keys(actual.fields)) {
        if (key in expected.fields) continue
        const childPath = path === '' ? key : `${path}.${key}`
        if (pathMatches(childPath, ignorePaths)) continue

        issues.push({
          path: childPath,
          kind: 'extra-key',
          expected: 'ausente',
          actual: describe(actual.fields[key]!),
          message: `${childPath}: chave a mais na resposta, ausente no golden`,
        })
      }
    }
  }

  return issues
}

export function formatIssues(issues: ShapeIssue[]): string {
  return issues.map((issue) => `  - ${issue.message}`).join('\n')
}
