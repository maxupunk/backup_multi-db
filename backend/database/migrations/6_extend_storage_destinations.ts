import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'storage_destinations'

  async up() {
    this.schema.alterTable(this.tableName, (table) => {
      table
        .string('provider', 50)
        .nullable()
        .comment(
          'Provider específico: aws_s3, minio, cloudflare_r2, google_gcs, azure_blob, sftp, local'
        )

      table.index(['provider'], 'idx_storage_destinations_provider')
    })

    // Backfill: preenche provider com base no type existente
    this.defer(async (db) => {
      const mapping: Record<string, string> = {
        local: 'local',
        s3: 'aws_s3',
        gcs: 'google_gcs',
        azure_blob: 'azure_blob',
        sftp: 'sftp',
      }

      for (const [type, provider] of Object.entries(mapping)) {
        await db.from(this.tableName).where('type', type).update({ provider })
      }
    })
  }

  async down() {
    this.schema.alterTable(this.tableName, (table) => {
      table.dropIndex(['provider'], 'idx_storage_destinations_provider')
      table.dropColumn('provider')
    })
  }
}
