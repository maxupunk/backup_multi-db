import vine from '@vinejs/vine'

const destinationTypes = ['local', 's3', 'gcs', 'azure_blob', 'sftp'] as const
const destinationStatuses = ['active', 'inactive'] as const

const createConfigGroup = vine.group([
  vine.group.if((data) => data.type === 'local', {
    type: vine.literal('local'),
    config: vine
      .object({
        basePath: vine.string().trim().optional(),
      })
      .optional(),
  }),
  vine.group.if((data) => data.type === 's3', {
    type: vine.literal('s3'),
    config: vine.object({
      region: vine.string().trim().minLength(1),
      bucket: vine.string().trim().minLength(1),
      endpoint: vine.string().trim().optional(),
      accessKeyId: vine.string().trim().minLength(1),
      secretAccessKey: vine.string().trim().minLength(1),
      forcePathStyle: vine.boolean().optional(),
      prefix: vine.string().trim().optional(),
    }),
  }),
  vine.group.if((data) => data.type === 'gcs', {
    type: vine.literal('gcs'),
    config: vine.object({
      bucket: vine.string().trim().minLength(1),
      projectId: vine.string().trim().optional(),
      credentialsJson: vine.string().trim().optional(),
      usingUniformAcl: vine.boolean().optional(),
      prefix: vine.string().trim().optional(),
    }),
  }),
  vine.group.if((data) => data.type === 'azure_blob', {
    type: vine.literal('azure_blob'),
    config: vine.object({
      connectionString: vine.string().trim().minLength(1),
      container: vine.string().trim().minLength(1),
      prefix: vine.string().trim().optional(),
    }),
  }),
  vine.group.if((data) => data.type === 'sftp', {
    type: vine.literal('sftp'),
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

export const createStorageDestinationValidator = vine.compile(
  vine
    .object({
      name: vine.string().trim().minLength(1).maxLength(100),
      status: vine.enum(destinationStatuses).optional(),
      isDefault: vine.boolean().optional(),
    })
    .merge(createConfigGroup)
)

const updateConfigGroup = vine
  .group([
    vine.group.if((data) => data.type === undefined && data.config === undefined, {}),
    vine.group.if((data) => data.type === 'local', {
      type: vine.literal('local'),
      config: vine
        .object({
          basePath: vine.string().trim().optional(),
        })
        .optional(),
    }),
    vine.group.if((data) => data.type === 's3', {
      type: vine.literal('s3'),
      config: vine.object({
        region: vine.string().trim().minLength(1),
        bucket: vine.string().trim().minLength(1),
        endpoint: vine.string().trim().optional(),
        accessKeyId: vine.string().trim().minLength(1),
        secretAccessKey: vine.string().trim().minLength(1),
        forcePathStyle: vine.boolean().optional(),
        prefix: vine.string().trim().optional(),
      }),
    }),
    vine.group.if((data) => data.type === 'gcs', {
      type: vine.literal('gcs'),
      config: vine.object({
        bucket: vine.string().trim().minLength(1),
        projectId: vine.string().trim().optional(),
        credentialsJson: vine.string().trim().optional(),
        usingUniformAcl: vine.boolean().optional(),
        prefix: vine.string().trim().optional(),
      }),
    }),
    vine.group.if((data) => data.type === 'azure_blob', {
      type: vine.literal('azure_blob'),
      config: vine.object({
        connectionString: vine.string().trim().minLength(1),
        container: vine.string().trim().minLength(1),
        prefix: vine.string().trim().optional(),
      }),
    }),
    vine.group.if((data) => data.type === 'sftp', {
      type: vine.literal('sftp'),
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
    field.report(
      'Quando enviar config, você deve informar type',
      'storage_destination.type_required',
      field
    )
  })

export const updateStorageDestinationValidator = vine.compile(
  vine
    .object({
      name: vine.string().trim().minLength(1).maxLength(100).optional(),
      status: vine.enum(destinationStatuses).optional(),
      isDefault: vine.boolean().optional(),
    })
    .merge(updateConfigGroup)
)

export const listStorageDestinationsValidator = vine.compile(
  vine.object({
    page: vine.number().positive().optional(),
    limit: vine.number().positive().max(100).optional(),
    type: vine.enum(destinationTypes).optional(),
    status: vine.enum(destinationStatuses).optional(),
    search: vine.string().trim().optional(),
  })
)
