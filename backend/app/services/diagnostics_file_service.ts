import { createReadStream, existsSync } from 'node:fs'
import { readdir, stat, unlink } from 'node:fs/promises'
import { basename, join, resolve, sep } from 'node:path'
import type { Readable } from 'node:stream'
import env from '#start/env'

export const DEFAULT_DIAGNOSTICS_PATH = '/storage/diagnostics'

/**
 * Extensoes reconhecidas como artefato de diagnostico do Node/V8.
 *
 * A lista e' uma allowlist: nada fora dela e' listado nem baixado, mesmo que
 * apareca no diretorio.
 */
const ALLOWED_EXTENSIONS = ['.heapsnapshot', '.cpuprofile', '.heapprofile'] as const

export type DiagnosticFile = {
  name: string
  sizeBytes: number
  createdAt: string
  modifiedAt: string
}

/**
 * Acesso aos artefatos de diagnostico gravados em `DIAGNOSTICS_PATH`.
 *
 * ATENCAO: um heap snapshot e' um despejo do heap inteiro do processo. Ele
 * contem, em texto claro, tudo que estava em memoria no momento da captura —
 * senhas de banco descriptografadas, credenciais de storage, tokens de sessao e
 * a chave de criptografia da aplicacao. Tratar como material mais sensivel que
 * um backup: acesso restrito a administradores e download registrado em
 * auditoria.
 */
export class DiagnosticsFileService {
  static getDirectory(): string {
    return env.get('DIAGNOSTICS_PATH') ?? DEFAULT_DIAGNOSTICS_PATH
  }

  static directoryExists(): boolean {
    return existsSync(this.getDirectory())
  }

  /**
   * Lista os artefatos disponiveis, do mais recente para o mais antigo.
   * Diretorio inexistente ou vazio devolve lista vazia — nao e' erro.
   */
  static async list(): Promise<DiagnosticFile[]> {
    const directory = this.getDirectory()

    if (!existsSync(directory)) {
      return []
    }

    const entries = await readdir(directory, { withFileTypes: true })
    const files: DiagnosticFile[] = []

    for (const entry of entries) {
      if (!entry.isFile() || !this.hasAllowedExtension(entry.name)) {
        continue
      }

      const stats = await stat(join(directory, entry.name))

      files.push({
        name: entry.name,
        sizeBytes: stats.size,
        createdAt: stats.birthtime.toISOString(),
        modifiedAt: stats.mtime.toISOString(),
      })
    }

    return files.sort((left, right) => right.modifiedAt.localeCompare(left.modifiedAt))
  }

  /**
   * Resolve o caminho absoluto de um artefato a partir do nome informado.
   *
   * Retorna `null` para qualquer entrada suspeita: nome com separador de
   * caminho, `..`, extensao fora da allowlist ou caminho que escape do
   * diretorio de diagnosticos. O parametro vem da URL, entao e' entrada de
   * usuario.
   */
  static resolvePath(fileName: string): string | null {
    const trimmed = (fileName ?? '').trim()

    if (!trimmed || trimmed !== basename(trimmed) || !this.hasAllowedExtension(trimmed)) {
      return null
    }

    const directory = resolve(this.getDirectory())
    const target = resolve(join(directory, trimmed))

    // Defesa em profundidade: mesmo com basename() aplicado, confirma que o
    // caminho final continua dentro do diretorio esperado.
    if (target !== directory && !target.startsWith(directory + sep)) {
      return null
    }

    return existsSync(target) ? target : null
  }

  static createReadStream(absolutePath: string): Readable {
    return createReadStream(absolutePath)
  }

  static async getSize(absolutePath: string): Promise<number> {
    const stats = await stat(absolutePath)
    return stats.size
  }

  static async remove(absolutePath: string): Promise<void> {
    await unlink(absolutePath)
  }

  private static hasAllowedExtension(fileName: string): boolean {
    const lower = fileName.toLowerCase()
    return ALLOWED_EXTENSIONS.some((extension) => lower.endsWith(extension))
  }
}
