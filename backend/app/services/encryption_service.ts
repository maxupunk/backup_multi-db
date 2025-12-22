import { createCipheriv, createDecipheriv, randomBytes } from 'node:crypto'
import env from '#start/env'

/**
 * Serviço de criptografia para senhas de banco de dados.
 * Utiliza AES-256-GCM para criptografia autenticada.
 *
 * O formato de armazenamento é: iv:authTag:encryptedData (todos em base64)
 */
export class EncryptionService {
  private static readonly ALGORITHM = 'aes-256-gcm'
  private static readonly IV_LENGTH = 16 // 128 bits
  private static readonly AUTH_TAG_LENGTH = 16 // 128 bits

  /**
   * Obtém a chave de criptografia do ambiente.
   * A chave deve ser um hex string de 64 caracteres (32 bytes).
   */
  private static getKey(): Buffer {
    const keyHex = env.get('DB_ENCRYPTION_KEY')

    if (!keyHex || keyHex.length !== 64) {
      throw new Error(
        'DB_ENCRYPTION_KEY deve ser definida no .env com 64 caracteres hexadecimais (32 bytes). ' +
          "Gere com: node -e \"console.log(require('crypto').randomBytes(32).toString('hex'))\""
      )
    }

    return Buffer.from(keyHex, 'hex')
  }

  /**
   * Criptografa um texto plano usando AES-256-GCM.
   *
   * @param plainText - Texto a ser criptografado
   * @returns String no formato iv:authTag:encryptedData (base64)
   */
  static encrypt(plainText: string): string {
    if (!plainText) {
      throw new Error('O texto para criptografia não pode estar vazio')
    }

    const key = this.getKey()
    const iv = randomBytes(this.IV_LENGTH)

    const cipher = createCipheriv(this.ALGORITHM, key, iv)

    let encrypted = cipher.update(plainText, 'utf8', 'base64')
    encrypted += cipher.final('base64')

    const authTag = cipher.getAuthTag()

    // Formato: iv:authTag:encryptedData (todos em base64)
    return [iv.toString('base64'), authTag.toString('base64'), encrypted].join(':')
  }

  /**
   * Descriptografa um texto criptografado com AES-256-GCM.
   *
   * @param encryptedText - String no formato iv:authTag:encryptedData (base64)
   * @returns Texto original descriptografado
   */
  static decrypt(encryptedText: string): string {
    if (!encryptedText) {
      throw new Error('O texto criptografado não pode estar vazio')
    }

    const parts = encryptedText.split(':')

    if (parts.length !== 3) {
      throw new Error('Formato de texto criptografado inválido. Esperado: iv:authTag:data')
    }

    const [ivBase64, authTagBase64, encrypted] = parts
    const key = this.getKey()

    const iv = Buffer.from(ivBase64, 'base64')
    const authTag = Buffer.from(authTagBase64, 'base64')

    // Validar tamanhos
    if (iv.length !== this.IV_LENGTH) {
      throw new Error(`IV inválido. Tamanho esperado: ${this.IV_LENGTH}, recebido: ${iv.length}`)
    }

    if (authTag.length !== this.AUTH_TAG_LENGTH) {
      throw new Error(
        `AuthTag inválido. Tamanho esperado: ${this.AUTH_TAG_LENGTH}, recebido: ${authTag.length}`
      )
    }

    const decipher = createDecipheriv(this.ALGORITHM, key, iv)
    decipher.setAuthTag(authTag)

    let decrypted = decipher.update(encrypted, 'base64', 'utf8')
    decrypted += decipher.final('utf8')

    return decrypted
  }

  /**
   * Verifica se uma string parece estar criptografada no formato esperado.
   *
   * @param text - Texto a verificar
   * @returns true se o formato parece válido
   */
  static isEncrypted(text: string): boolean {
    if (!text) return false

    const parts = text.split(':')
    if (parts.length !== 3) return false

    try {
      const iv = Buffer.from(parts[0], 'base64')
      const authTag = Buffer.from(parts[1], 'base64')

      return iv.length === this.IV_LENGTH && authTag.length === this.AUTH_TAG_LENGTH
    } catch {
      return false
    }
  }
}
