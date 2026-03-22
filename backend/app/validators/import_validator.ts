import vine from '@vinejs/vine'

/**
 * Validator para importação de arquivos de backup
 */
export const importBackupValidator = vine.compile(
  vine.object({
    /** ID da conexão de banco de dados a associar ao backup importado */
    connectionId: vine.number().positive(),
    /** Nome do banco de dados ao qual o backup pertence */
    databaseName: vine.string().trim().minLength(1).maxLength(255),
    /** Se deve verificar a integridade do arquivo antes de importar */
    verifyIntegrity: vine
      .boolean()
      .optional()
      .transform((v) => v ?? false),
  })
)
