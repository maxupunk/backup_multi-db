import { existsSync, statfsSync } from 'node:fs'
import { StorageDestinationService } from '#services/storage_destination_service'
import StorageDestination from '#models/storage_destination'
import logger from '@adonisjs/core/services/logger'

/**
 * Informações de espaço de armazenamento
 */
export interface StorageSpaceInfo {
  destinationId: number | null
  destinationName: string
  type: string
  totalBytes: number
  usedBytes: number
  freeBytes: number
  usedPercent: number
  freePercent: number
  isLowSpace: boolean
  lowSpaceThreshold: number
}

/**
 * Resultado da verificação de espaço antes do backup
 */
export interface SpaceCheckResult {
  hasEnoughSpace: boolean
  freePercent: number
  freeBytes: number
  warning?: string
}

/**
 * Serviço para verificação de espaço em armazenamentos
 */
export class StorageSpaceService {
  private static readonly LOW_SPACE_THRESHOLD_PERCENT = 10

  /**
   * Obtém informações de espaço do sistema de arquivos local
   */
  static getLocalStorageSpace(path: string): { total: number; free: number } | null {
    try {
      if (!existsSync(path)) {
        return null
      }

      const stats = statfsSync(path)
      const blockSize = stats.bsize
      const totalBlocks = stats.blocks
      const freeBlocks = stats.bfree

      return {
        total: totalBlocks * blockSize,
        free: freeBlocks * blockSize,
      }
    } catch (error) {
      logger.warn(`[StorageSpace] Erro ao obter espaço do path ${path}: ${error}`)
      return null
    }
  }

  /**
   * Obtém informações de espaço para um destino específico
   */
  static async getDestinationSpaceInfo(
    destination: StorageDestination | null
  ): Promise<StorageSpaceInfo | null> {
    const basePath = StorageDestinationService.getLocalBasePath(destination)
    const spaceInfo = this.getLocalStorageSpace(basePath)

    if (!spaceInfo) {
      return null
    }

    const usedBytes = spaceInfo.total - spaceInfo.free
    const usedPercent = spaceInfo.total > 0 ? (usedBytes / spaceInfo.total) * 100 : 0
    const freePercent = 100 - usedPercent

    const config = destination?.getDecryptedConfig()
    const isLocal = !destination || config?.type === 'local'

    // Para destinos remotos (S3, GCS, etc.), retornamos null pois não temos como verificar espaço
    if (!isLocal) {
      return null
    }

    return {
      destinationId: destination?.id ?? null,
      destinationName: destination?.name ?? 'Local (padrão)',
      type: destination?.type ?? 'local',
      totalBytes: spaceInfo.total,
      usedBytes,
      freeBytes: spaceInfo.free,
      usedPercent: Math.round(usedPercent * 100) / 100,
      freePercent: Math.round(freePercent * 100) / 100,
      isLowSpace: freePercent < this.LOW_SPACE_THRESHOLD_PERCENT,
      lowSpaceThreshold: this.LOW_SPACE_THRESHOLD_PERCENT,
    }
  }

  /**
   * Obtém informações de espaço de todos os destinos de armazenamento ativos
   */
  static async getAllDestinationsSpaceInfo(): Promise<StorageSpaceInfo[]> {
    const destinations = await StorageDestination.query()
      .where('status', 'active')
      .orderBy('name', 'asc')

    const results: StorageSpaceInfo[] = []

    // Adiciona o armazenamento local padrão
    const defaultSpaceInfo = await this.getDestinationSpaceInfo(null)
    if (defaultSpaceInfo) {
      // Verifica se já não existe um destino local padrão cadastrado
      const hasDefaultLocal = destinations.some(
        (d) => d.isDefault && d.type === 'local'
      )
      if (!hasDefaultLocal) {
        results.push(defaultSpaceInfo)
      }
    }

    // Adiciona os destinos cadastrados
    for (const destination of destinations) {
      const spaceInfo = await this.getDestinationSpaceInfo(destination)
      if (spaceInfo) {
        results.push(spaceInfo)
      }
    }

    return results
  }

  /**
   * Verifica se há espaço suficiente para realizar um backup
   * Retorna warning se espaço livre for menor que 10%
   */
  static async checkSpaceBeforeBackup(
    destination: StorageDestination | null
  ): Promise<SpaceCheckResult> {
    const spaceInfo = await this.getDestinationSpaceInfo(destination)

    // Se não conseguir obter informações (destino remoto ou erro), assume que há espaço
    if (!spaceInfo) {
      return {
        hasEnoughSpace: true,
        freePercent: 100,
        freeBytes: 0,
      }
    }

    const result: SpaceCheckResult = {
      hasEnoughSpace: true,
      freePercent: spaceInfo.freePercent,
      freeBytes: spaceInfo.freeBytes,
    }

    if (spaceInfo.isLowSpace) {
      result.warning = `Espaço em disco baixo no armazenamento "${spaceInfo.destinationName}": ` +
        `apenas ${spaceInfo.freePercent.toFixed(1)}% livre (${this.formatBytes(spaceInfo.freeBytes)}). ` +
        `Recomenda-se ter pelo menos ${this.LOW_SPACE_THRESHOLD_PERCENT}% de espaço livre.`

      logger.warn(`[StorageSpace] ${result.warning}`)
    }

    return result
  }

  /**
   * Formata bytes para exibição legível
   */
  static formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B'

    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))

    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`
  }
}
