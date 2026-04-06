import { DateTime } from 'luxon'
import { BaseModel, column } from '@adonisjs/lucid/orm'

export type ResourceMetricScope = 'system' | 'container'

export default class ResourceMetricHistory extends BaseModel {
  public static table = 'resource_metric_history'

  @column({ isPrimary: true })
  declare id: number

  @column()
  declare scope: ResourceMetricScope

  @column()
  declare entityId: string | null

  @column()
  declare entityName: string | null

  @column()
  declare cpuUsagePercent: number

  @column()
  declare memoryUsagePercent: number

  @column()
  declare memoryUsedBytes: number

  @column()
  declare memoryTotalBytes: number

  @column.dateTime()
  declare collectedAt: DateTime

  @column.dateTime({ autoCreate: true })
  declare createdAt: DateTime

  @column.dateTime({ autoCreate: true, autoUpdate: true })
  declare updatedAt: DateTime
}
