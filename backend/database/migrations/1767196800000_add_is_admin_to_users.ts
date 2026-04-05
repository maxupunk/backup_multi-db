import { BaseSchema } from '@adonisjs/lucid/schema'

export default class extends BaseSchema {
  protected tableName = 'users'

  async up() {
    const hasIsAdmin = await this.schema.hasColumn(this.tableName, 'is_admin')
    const hasIsActive = await this.schema.hasColumn(this.tableName, 'is_active')

    if (!hasIsAdmin) {
      try {
        await this.schema.alterTable(this.tableName, (table) => {
          table.boolean('is_admin').defaultTo(false).notNullable()
        })
      } catch (error) {
        const columnExistsNow = await this.schema.hasColumn(this.tableName, 'is_admin')
        if (!columnExistsNow || !this.isDuplicateIsAdminError(error)) {
          throw error
        }
      }
    }

    const firstActiveUser = hasIsActive
      ? await this.db
          .from(this.tableName)
          .select('id')
          .where('is_active', true)
          .orderBy('id', 'asc')
          .first()
      : null

    const firstUser =
      firstActiveUser ||
      (await this.db.from(this.tableName).select('id').orderBy('id', 'asc').first())

    if (firstUser?.id) {
      const updatePayload: Record<string, boolean> = { is_admin: true }

      if (hasIsActive) {
        updatePayload.is_active = true
      }

      await this.db
        .from(this.tableName)
        .where('id', firstUser.id)
        .update({
          ...updatePayload,
        })
    }
  }

  async down() {
    const hasIsAdmin = await this.schema.hasColumn(this.tableName, 'is_admin')

    if (!hasIsAdmin) {
      return
    }

    await this.schema.alterTable(this.tableName, (table) => {
      table.dropColumn('is_admin')
    })
  }

  private isDuplicateIsAdminError(error: unknown): boolean {
    return error instanceof Error && /duplicate column name:\s*is_admin/i.test(error.message)
  }
}
