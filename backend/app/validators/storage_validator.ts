import vine from '@vinejs/vine'

const storageProviders = [
  'aws_s3',
  'minio',
  'cloudflare_r2',
  'google_gcs',
  'azure_blob',
  'sftp',
  'local',
] as const

const storageTypes = ['local', 's3', 'gcs', 'azure_blob', 'sftp'] as const
const storageStatuses = ['active', 'inactive'] as const

// ==================== Create Storage ====================

const createConfigGroup = vine.group([
  vine.group.if((data) => data.provider === 'local', {
    provider: vine.literal('local'),
    config: vine
      .object({
        basePath: vine.string().trim().optional(),
      })
      .optional(),
  }),
  vine.group.if(
    (data) =>
      data.provider === 'aws_s3' || data.provider === 'minio' || data.provider === 'cloudflare_r2',
    {
      provider: vine.enum(['aws_s3', 'minio', 'cloudflare_r2'] as const),
      config: vine.object({
        region: vine.string().trim().minLength(1),
        bucket: vine.string().trim().minLength(1),
        endpoint: vine.string().trim().optional(),
        accessKeyId: vine.string().trim().minLength(1),
        secretAccessKey: vine.string().trim().minLength(1),
        forcePathStyle: vine.boolean().optional(),
        prefix: vine.string().trim().optional(),
      }),
    }
  ),
  vine.group.if((data) => data.provider === 'google_gcs', {
    provider: vine.literal('google_gcs'),
    config: vine.object({
      bucket: vine.string().trim().minLength(1),
      projectId: vine.string().trim().optional(),
      credentialsJson: vine.string().trim().optional(),
      prefix: vine.string().trim().optional(),
    }),
  }),
  vine.group.if((data) => data.provider === 'azure_blob', {
    provider: vine.literal('azure_blob'),
    config: vine.object({
      connectionString: vine.string().trim().minLength(1),
      container: vine.string().trim().minLength(1),
      prefix: vine.string().trim().optional(),
    }),
  }),
  vine.group.if((data) => data.provider === 'sftp', {
    provider: vine.literal('sftp'),
    config: vine.object({
      host: vine.string().trim().minLength(1),
      port: vine.number().positive().max(65535).optional(),
      username: vine.string().trim().minLength(1),
      password: vine.string().trim().optional(),
      privateKey: vine.string().trim().optional(),
      passphrase: vine.string().trim().optional(),
      basePath: vine.string().trim().optional(),
    }),
  }),
])

export const createStorageValidator = vine.compile(
  vine
    .object({
      name: vine.string().trim().minLength(1).maxLength(100),
      status: vine.enum(storageStatuses).optional(),
      isDefault: vine.boolean().optional(),
    })
    .merge(createConfigGroup)
)

// ==================== Update Storage ====================

const updateConfigGroup = vine
  .group([
    vine.group.if((data) => data.provider === undefined && data.config === undefined, {}),
    vine.group.if((data) => data.provider === 'local', {
      provider: vine.literal('local'),
      config: vine
        .object({
          basePath: vine.string().trim().optional(),
        })
        .optional(),
    }),
    vine.group.if(
      (data) =>
        data.provider === 'aws_s3' ||
        data.provider === 'minio' ||
        data.provider === 'cloudflare_r2',
      {
        provider: vine.enum(['aws_s3', 'minio', 'cloudflare_r2'] as const),
        config: vine.object({
          region: vine.string().trim().minLength(1),
          bucket: vine.string().trim().minLength(1),
          endpoint: vine.string().trim().optional(),
          accessKeyId: vine.string().trim().minLength(1),
          secretAccessKey: vine.string().trim().minLength(1),
          forcePathStyle: vine.boolean().optional(),
          prefix: vine.string().trim().optional(),
        }),
      }
    ),
    vine.group.if((data) => data.provider === 'google_gcs', {
      provider: vine.literal('google_gcs'),
      config: vine.object({
        bucket: vine.string().trim().minLength(1),
        projectId: vine.string().trim().optional(),
        credentialsJson: vine.string().trim().optional(),
        prefix: vine.string().trim().optional(),
      }),
    }),
    vine.group.if((data) => data.provider === 'azure_blob', {
      provider: vine.literal('azure_blob'),
      config: vine.object({
        connectionString: vine.string().trim().minLength(1),
        container: vine.string().trim().minLength(1),
        prefix: vine.string().trim().optional(),
      }),
    }),
    vine.group.if((data) => data.provider === 'sftp', {
      provider: vine.literal('sftp'),
      config: vine.object({
        host: vine.string().trim().minLength(1),
        port: vine.number().positive().max(65535).optional(),
        username: vine.string().trim().minLength(1),
        password: vine.string().trim().optional(),
        privateKey: vine.string().trim().optional(),
        passphrase: vine.string().trim().optional(),
        basePath: vine.string().trim().optional(),
      }),
    }),
  ])
  .otherwise((_, field) => {
    field.report('Quando enviar config, informe o provider', 'storage.provider_required', field)
  })

export const updateStorageValidator = vine.compile(
  vine
    .object({
      name: vine.string().trim().minLength(1).maxLength(100).optional(),
      status: vine.enum(storageStatuses).optional(),
      isDefault: vine.boolean().optional(),
    })
    .merge(updateConfigGroup)
)

// ==================== Browse Storage ====================

export const browseStorageValidator = vine.compile(
  vine.object({
    path: vine.string().trim().optional(),
    cursor: vine.string().trim().optional(),
    limit: vine.number().positive().max(1000).optional(),
    prefix: vine.string().trim().optional(),
  })
)

// ==================== Copy Storage ====================

export const copyStorageValidator = vine.compile(
  vine.object({
    destinationId: vine.number().positive(),
    sourcePath: vine.string().trim().optional(),
    destinationPath: vine.string().trim().optional(),
    dryRun: vine.boolean().optional(),
    deleteExtraneous: vine.boolean().optional(),
  })
)

// ==================== List Storages ====================

export const listStoragesValidator = vine.compile(
  vine.object({
    page: vine.number().positive().optional(),
    limit: vine.number().positive().max(100).optional(),
    type: vine.enum(storageTypes).optional(),
    provider: vine.enum(storageProviders).optional(),
    status: vine.enum(storageStatuses).optional(),
    search: vine.string().trim().optional(),
  })
)

// ==================== Archive Storage ====================

export const archiveStorageValidator = vine.compile(
  vine.object({
    path: vine.string().trim().optional(),
  })
)
