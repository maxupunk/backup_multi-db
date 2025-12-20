import vine from '@vinejs/vine'

/**
 * Validator para registro de usuários
 */
export const registerValidator = vine.compile(
  vine.object({
    fullName: vine.string().maxLength(100).optional(),
    email: vine
      .string()
      .email()
      .normalizeEmail()
      .unique(async (db, value) => {
        const match = await db.from('users').select('id').where('email', value).first()
        return !match
      }),
    password: vine.string().minLength(8).maxLength(32),
  })
)

/**
 * Validator para login
 */
export const loginValidator = vine.compile(
  vine.object({
    email: vine.string().email().normalizeEmail(),
    password: vine.string(),
  })
)