import {
  createReadStream,
  createWriteStream,
  openSync,
  readSync,
  closeSync,
  statSync,
} from 'node:fs'
import { mkdir, rename, unlink } from 'node:fs/promises'
import { pipeline } from 'node:stream/promises'
import { createHash } from 'node:crypto'
import { createGunzip } from 'node:zlib'
import { join, extname } from 'node:path'
import type { MultipartFile } from '@adonisjs/core/types/bodyparser'
import app from '@adonisjs/core/services/app'
import env from '#start/env'
import logger from '@adonisjs/core/services/logger'
import Backup from '#models/backup'
import Connection from '#models/connection'

// ─── Tipos ───────────────────────────────────────────────────────────────────

/** Formatos de arquivo de backup suportados para importação */
export type ImportedFileFormat = 'sql' | 'sql.gz' | 'dump' | 'zip' | 'tar'

export interface ImportOptions {
  connectionId: number
  databaseName: string
  verifyIntegrity: boolean
}

export interface IntegrityCheckResult {
  valid: boolean
  format: ImportedFileFormat
  message: string
  warnings?: string[]
}

export interface ImportResult {
  backup: Backup
  format: ImportedFileFormat
  checksum: string
  fileSize: number
  integrityResult?: IntegrityCheckResult
}

// ─── Constantes ──────────────────────────────────────────────────────────────

/** Mapa de extensão → formato canônico */
const EXTENSION_FORMAT_MAP: Record<string, ImportedFileFormat> = {
  '.sql': 'sql',
  '.sql.gz': 'sql.gz',
  '.gz': 'sql.gz',
  '.dump': 'dump',
  '.pgdump': 'dump',
  '.pg_dump': 'dump',
  '.zip': 'zip',
  '.tar': 'tar',
  '.tar.gz': 'tar',
  '.tgz': 'tar',
}

/** Extensões aceitas para importação */
const ACCEPTED_EXTENSIONS = new Set(Object.keys(EXTENSION_FORMAT_MAP))

/** Magic bytes para identificação de formato */
const MAGIC = {
  GZIP: Buffer.from([0x1f, 0x8b]),
  ZIP: Buffer.from([0x50, 0x4b, 0x03, 0x04]),
  PGDUMP: Buffer.from('PGDMP'),
} as const

/** Padrões de instrução SQL válida */
const SQL_PATTERN = /\b(CREATE|INSERT|DROP|ALTER|SELECT|COPY|SET\s|BEGIN|COMMIT|--)\b/i

// ─── Serviço ─────────────────────────────────────────────────────────────────

/**
 * Serviço responsável por importar arquivos de backup externos para o sistema.
 *
 * Formatos suportados:
 * - `.sql`              → SQL texto plano
 * - `.sql.gz` / `.gz`  → SQL comprimido com Gzip
 * - `.dump` / `.pgdump`→ PostgreSQL custom format (pg_dump -Fc)
 * - `.zip`             → Arquivo ZIP (validação via magic bytes)
 * - `.tar` / `.tar.gz` / `.tgz` → TAR ou TAR.GZ (validação via magic bytes)
 */
export class BackupImportService {
  private readonly storagePath: string

  constructor() {
    this.storagePath = env.get('BACKUP_STORAGE_PATH') ?? app.makePath('storage/backups')
  }

  // ─── Public API ────────────────────────────────────────────────────────────

  async import(
    file: MultipartFile,
    connection: Connection,
    options: ImportOptions
  ): Promise<ImportResult> {
    this.validateFilePresence(file)

    const tmpPath = file.tmpPath!
    const originalName = file.clientName
    const format = this.detectFormat(originalName, tmpPath)

    logger.info(
      `[Import] Importando arquivo "${originalName}" (${format}) ` +
        `para conexão "${connection.name}" (ID: ${connection.id}), database: "${options.databaseName}"`
    )

    // Checksum antes de mover o arquivo
    const checksum = await this.calculateChecksum(tmpPath)
    const fileSize = statSync(tmpPath).size

    // Verificação de integridade (opcional)
    let integrityResult: IntegrityCheckResult | undefined
    if (options.verifyIntegrity) {
      integrityResult = await this.verifyIntegrity(tmpPath, format)

      if (!integrityResult.valid) {
        logger.warn(`[Import] Falha na verificação de integridade: ${integrityResult.message}`)
        throw new Error(`Falha na verificação de integridade: ${integrityResult.message}`)
      }

      logger.info(`[Import] Integridade verificada: ${integrityResult.message}`)
    }

    // Mover para o diretório de armazenamento
    const { destPath, finalFileName, relPath } = await this.moveToStorage(
      tmpPath,
      connection.id,
      originalName
    )

    logger.info(`[Import] Arquivo movido para: ${destPath}`)

    // Criar registro de backup
    const backup = await Backup.create({
      connectionId: connection.id,
      connectionDatabaseId: null,
      databaseName: options.databaseName,
      storageDestinationId: null,
      status: 'completed',
      filePath: relPath,
      fileName: finalFileName,
      fileSize,
      checksum,
      compressed: format === 'sql.gz',
      retentionType: 'daily',
      protected: false,
      trigger: 'manual',
      metadata: {
        isImported: true,
        originalFileName: originalName,
        format,
        integrityVerified: options.verifyIntegrity ? (integrityResult?.valid ?? false) : false,
        warnings: integrityResult?.warnings,
      },
    })

    logger.info(`[Import] Backup registrado com ID: ${backup.id}`)

    return { backup, format, checksum, fileSize, integrityResult }
  }

  // ─── Validação de arquivo ──────────────────────────────────────────────────

  private validateFilePresence(file: MultipartFile): void {
    if (!file.tmpPath) {
      throw new Error('Nenhum arquivo encontrado na requisição')
    }

    const format = this.detectFormat(file.clientName, file.tmpPath)

    if (!ACCEPTED_EXTENSIONS.has(this.resolveExtension(file.clientName))) {
      throw new Error(
        `Formato de arquivo não suportado. ` +
          `Formatos aceitos: .sql, .sql.gz, .gz, .dump, .pgdump, .zip, .tar, .tar.gz, .tgz`
      )
    }

    void format // utilizado apenas para garantir que o mapeamento existe
  }

  // ─── Detecção de formato ────────────────────────────────────────────────────

  private resolveExtension(fileName: string): string {
    const lower = fileName.toLowerCase()

    if (lower.endsWith('.sql.gz')) return '.sql.gz'
    if (lower.endsWith('.tar.gz')) return '.tar.gz'

    return extname(lower)
  }

  private detectFormat(fileName: string, filePath: string): ImportedFileFormat {
    const ext = this.resolveExtension(fileName)

    // Se a extensão for ambígua (.gz sem .sql prefix), confirma via magic bytes
    if (ext === '.gz') {
      const header = this.readFileHeader(filePath, 2)
      if (!header.subarray(0, 2).equals(MAGIC.GZIP)) {
        throw new Error('O arquivo tem extensão .gz mas não é um arquivo Gzip válido')
      }
      return 'sql.gz'
    }

    const format = EXTENSION_FORMAT_MAP[ext]

    if (!format) {
      throw new Error(
        `Extensão não suportada: "${ext}". ` +
          `Formatos aceitos: .sql, .sql.gz, .gz, .dump, .pgdump, .zip, .tar, .tar.gz, .tgz`
      )
    }

    return format
  }

  // ─── Verificação de integridade ────────────────────────────────────────────

  async verifyIntegrity(
    filePath: string,
    format: ImportedFileFormat
  ): Promise<IntegrityCheckResult> {
    try {
      switch (format) {
        case 'sql':
          return this.verifySqlFile(filePath)
        case 'sql.gz':
          return this.verifyGzipFile(filePath)
        case 'dump':
          return this.verifyPostgresCustomFormat(filePath)
        case 'zip':
          return this.verifyZipFile(filePath)
        case 'tar':
          return this.verifyTarFile(filePath)
      }
    } catch (error) {
      return {
        valid: false,
        format,
        message: error instanceof Error ? error.message : 'Erro desconhecido durante verificação',
      }
    }
  }

  private verifySqlFile(filePath: string): IntegrityCheckResult {
    const header = this.readFileHeader(filePath, 8192)
    const text = header.toString('utf-8')

    if (!SQL_PATTERN.test(text)) {
      return {
        valid: false,
        format: 'sql',
        message:
          'O arquivo não contém instruções SQL reconhecíveis no início (esperado: CREATE, INSERT, COPY, etc.)',
      }
    }

    return { valid: true, format: 'sql', message: 'Arquivo SQL válido' }
  }

  private async verifyGzipFile(filePath: string): Promise<IntegrityCheckResult> {
    const header = this.readFileHeader(filePath, 2)

    if (!header.subarray(0, 2).equals(MAGIC.GZIP)) {
      return {
        valid: false,
        format: 'sql.gz',
        message: 'Magic bytes inválidos — o arquivo não é um Gzip válido (esperado: 0x1F 0x8B)',
      }
    }

    try {
      await this.probeGzipDecompression(filePath)
      return { valid: true, format: 'sql.gz', message: 'Arquivo Gzip válido e decomprimível' }
    } catch (error) {
      return {
        valid: false,
        format: 'sql.gz',
        message: `O arquivo Gzip está corrompido: ${error instanceof Error ? error.message : 'erro desconhecido'}`,
      }
    }
  }

  private verifyPostgresCustomFormat(filePath: string): IntegrityCheckResult {
    const header = this.readFileHeader(filePath, 8)

    if (!header.subarray(0, 5).equals(MAGIC.PGDUMP)) {
      return {
        valid: false,
        format: 'dump',
        message:
          'Magic bytes inválidos — o arquivo não é um dump PostgreSQL no formato customizado (esperado: PGDMP)',
      }
    }

    return {
      valid: true,
      format: 'dump',
      message: 'Dump PostgreSQL no formato customizado (pg_dump -Fc) válido',
    }
  }

  private verifyZipFile(filePath: string): IntegrityCheckResult {
    const header = this.readFileHeader(filePath, 4)

    if (!header.subarray(0, 4).equals(MAGIC.ZIP)) {
      return {
        valid: false,
        format: 'zip',
        message: 'Magic bytes inválidos — o arquivo não é um ZIP válido (esperado: PK\\x03\\x04)',
      }
    }

    return {
      valid: true,
      format: 'zip',
      message: 'Arquivo ZIP válido',
      warnings: [
        'Arquivos ZIP importados requerem extração prévia para serem restaurados pela ferramenta de restore',
      ],
    }
  }

  private verifyTarFile(filePath: string): IntegrityCheckResult {
    const header = this.readFileHeader(filePath, 265)

    // Gzip-compressed TAR (.tar.gz / .tgz): primeiro verifica magic gzip
    if (header.subarray(0, 2).equals(MAGIC.GZIP)) {
      return {
        valid: true,
        format: 'tar',
        message: 'Arquivo TAR.GZ (Gzip) válido',
        warnings: [
          'Arquivos TAR importados requerem extração prévia para serem restaurados pela ferramenta de restore',
        ],
      }
    }

    // POSIX/GNU TAR: magic "ustar" começa no offset 257
    const ustar = header.subarray(257, 262).toString('ascii')
    if (!ustar.startsWith('ustar')) {
      return {
        valid: false,
        format: 'tar',
        message: 'O arquivo não é um TAR válido (magic "ustar" não encontrado no offset 257)',
      }
    }

    return {
      valid: true,
      format: 'tar',
      message: 'Arquivo TAR válido',
      warnings: [
        'Arquivos TAR importados requerem extração prévia para serem restaurados pela ferramenta de restore',
      ],
    }
  }

  // ─── Utilitários ───────────────────────────────────────────────────────────

  /**
   * Lê os primeiros N bytes de um arquivo de forma síncrona.
   * Eficiente para verificação de magic bytes sem carregar o arquivo inteiro.
   */
  private readFileHeader(filePath: string, bytes: number): Buffer {
    const buf = Buffer.alloc(bytes)
    const fd = openSync(filePath, 'r')
    try {
      const bytesRead = readSync(fd, buf, 0, bytes, 0)
      return buf.subarray(0, bytesRead)
    } finally {
      closeSync(fd)
    }
  }

  /**
   * Tenta descomprimir o início do arquivo Gzip para confirmar validade.
   * Usa apenas os primeiros 64KB para não bloquear em arquivos grandes.
   */
  private probeGzipDecompression(filePath: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const readStream = createReadStream(filePath, { start: 0, end: 65535 })
      const gunzip = createGunzip()
      let resolved = false

      const done = () => {
        if (!resolved) {
          resolved = true
          resolve()
        }
      }

      gunzip.on('data', () => {
        readStream.destroy()
        gunzip.destroy()
        done()
      })

      gunzip.on('error', (err) => {
        if (!resolved) {
          resolved = true
          reject(err)
        }
      })

      readStream.on('error', (err) => {
        if (!resolved) {
          resolved = true
          reject(err)
        }
      })

      readStream.pipe(gunzip)
    })
  }

  /**
   * Calcula o SHA-256 do arquivo completo.
   */
  async calculateChecksum(filePath: string): Promise<string> {
    const hash = createHash('sha256')

    await new Promise<void>((resolve, reject) => {
      const stream = createReadStream(filePath)
      stream.on('data', (chunk) => hash.update(chunk))
      stream.on('end', resolve)
      stream.on('error', reject)
    })

    return hash.digest('hex')
  }

  /**
   * Move o arquivo do diretório temporário para o diretório de armazenamento.
   * O nome final inclui timestamp para evitar colisões.
   *
   * Tenta primeiro `rename()` (operação atômica e eficiente quando estão no
   * mesmo dispositivo). Se falhar com EXDEV (cross-device, comum em Docker
   * onde /tmp e o volume de dados são filesystems distintos), faz cópia via
   * stream e remove o original — comportamento equivalente ao `mv` do shell.
   */
  private async moveToStorage(
    tmpPath: string,
    connectionId: number,
    originalName: string
  ): Promise<{ destPath: string; finalFileName: string; relPath: string }> {
    const destDir = join(this.storagePath, String(connectionId))
    await mkdir(destDir, { recursive: true })

    const finalFileName = `import_${Date.now()}_${originalName}`
    const destPath = join(destDir, finalFileName)

    // Caminho relativo separado por / (portável para banco e storage remoto)
    const relPath = `${connectionId}/${finalFileName}`

    try {
      await rename(tmpPath, destPath)
    } catch (err) {
      // EXDEV = cross-device rename (e.g. /tmp → volume mount no Docker)
      if ((err as NodeJS.ErrnoException).code !== 'EXDEV') throw err

      await pipeline(createReadStream(tmpPath), createWriteStream(destPath))
      await unlink(tmpPath)
    }

    return { destPath, finalFileName, relPath }
  }
}
