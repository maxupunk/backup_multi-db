import vine from '@vinejs/vine'

export const updateBackupRetentionPolicyValidator = vine.compile(
  vine.object({
    daily: vine.number().min(0).max(3650),
    weekly: vine.number().min(0).max(520),
    monthly: vine.number().min(0).max(240),
    yearly: vine.number().min(0).max(100),
    pruneCron: vine.string().trim().minLength(1).maxLength(100),
  })
)
