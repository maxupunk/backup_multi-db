import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'users'

  async up() {
    this.schema.alterTable(this.tableName, (table) => {
      table.boolean('is_admin').defaultTo(false).notNullable()
    })

    const firstActiveUser = await this.db
      .from(this.tableName)
      .select('id')
      .where('is_active', true)
      .orderBy('id', 'asc')
      .first()

    const firstUser =
      firstActiveUser ||
      (await this.db.from(this.tableName).select('id').orderBy('id', 'asc').first())

    if (firstUser?.id) {
      await this.db.from(this.tableName).where('id', firstUser.id).update({
        is_admin: true,
        is_active: true,
      })
    }
  }

  async down() {
    this.schema.alterTable(this.tableName, (table) => {
      table.dropColumn('is_admin')
    })
  }
}