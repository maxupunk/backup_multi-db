import { describe, expect, it } from 'vitest'
import { compareShapes, shapeOf } from '../../src/shape.ts'

const issues = (actual: unknown, expected: unknown, options = {}) =>
  compareShapes(shapeOf(actual), shapeOf(expected), options)

describe('shapeOf / compareShapes', () => {
  it('ignora a ordem das chaves', () => {
    expect(issues({ b: 1, a: 'x' }, { a: 'y', b: 2 })).toEqual([])
  })

  it('ignora o valor e compara o tipo', () => {
    expect(issues({ id: 999, name: 'outro' }, { id: 1, name: 'algum' })).toEqual([])
  })

  it('acusa troca de tipo', () => {
    const found = issues({ id: '1' }, { id: 1 })
    expect(found).toHaveLength(1)
    expect(found[0]).toMatchObject({ kind: 'type-mismatch', path: 'id' })
  })

  it('acusa chave ausente', () => {
    const found = issues({ id: 1 }, { id: 1, email: 'a@b.c' })
    expect(found).toHaveLength(1)
    expect(found[0]).toMatchObject({ kind: 'missing-key', path: 'email' })
  })

  it('acusa chave a mais', () => {
    const found = issues({ id: 1, extra: true }, { id: 1 })
    expect(found).toHaveLength(1)
    expect(found[0]).toMatchObject({ kind: 'extra-key', path: 'extra' })
  })

  it('tolera chave a mais quando pedido explicitamente', () => {
    expect(issues({ id: 1, extra: true }, { id: 1 }, { allowExtraKeys: true })).toEqual([])
  })

  it('trata null como nulabilidade, nao como conflito', () => {
    // Nos dois sentidos: o golden gravou null e a resposta trouxe valor, e
    // vice-versa. Nenhum dos dois e' quebra de contrato — e' campo opcional
    // que calhou de estar preenchido (ou nao) na hora da gravacao.
    expect(issues({ storageId: 3 }, { storageId: null })).toEqual([])
    expect(issues({ storageId: null }, { storageId: 3 })).toEqual([])
  })

  it('desce na estrutura aninhada', () => {
    const found = issues(
      { data: { user: { id: 1, isAdmin: 'sim' } } },
      { data: { user: { id: 1, isAdmin: true } } }
    )
    expect(found).toHaveLength(1)
    expect(found[0]).toMatchObject({ kind: 'type-mismatch', path: 'data.user.isAdmin' })
  })

  it('compara o formato do item do array, nao o tamanho', () => {
    expect(
      issues({ data: [{ id: 1 }, { id: 2 }, { id: 3 }] }, { data: [{ id: 9 }] })
    ).toEqual([])
  })

  it('une itens heterogeneos em vez de olhar so o primeiro', () => {
    // O item 0 nao tem `nota`; o item 1 tem. Se a derivacao olhasse so o
    // primeiro, o campo sumiria do contrato sem ninguem notar.
    const shape = shapeOf([{ id: 1 }, { id: 2, nota: 'x' }])
    expect(shape.kind).toBe('array')
    if (shape.kind !== 'array' || !shape.element || shape.element.kind !== 'object') {
      throw new Error('formato inesperado')
    }
    expect(Object.keys(shape.element.fields).sort()).toEqual(['id', 'nota'])
  })

  it('sinaliza array vazio como nao verificado, em vez de aprovar em silencio', () => {
    const found = issues({ data: [] }, { data: [{ id: 1 }] })
    expect(found).toHaveLength(1)
    expect(found[0]).toMatchObject({ kind: 'unverified-array', path: 'data' })
  })

  it('nao exige item quando o proprio golden gravou array vazio', () => {
    expect(issues({ data: [{ id: 1 }] }, { data: [] })).toEqual([])
  })

  it('respeita ignorePaths, inclusive com curinga', () => {
    expect(
      issues(
        { data: [{ id: 1, config: { a: 1 } }] },
        { data: [{ id: 1, config: { b: 'x' } }] },
        { ignorePaths: ['data.*.config'] }
      )
    ).toEqual([])
  })

  it('nao aprova tudo — o matcher precisa reprovar algo', () => {
    // Guarda contra a falha mais perigosa possivel deste modulo: um matcher
    // que sempre devolve lista vazia faria todos os testes acima passarem
    // menos este.
    expect(issues({ a: 1 }, { b: 'x' }).length).toBeGreaterThan(0)
  })
})
