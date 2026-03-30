import type { HttpContext } from '@adonisjs/core/http'
import StorageDestination from '#models/storage_destination'
import type { StorageDestinationType, StorageProvider } from '#models/storage_destination'
import { BucketExplorerService } from '#services/storage/bucket_explorer_service'
import { BucketCopyService } from '#services/storage/bucket_copy_service'
import { BucketArchiveService } from '#services/storage/bucket_archive_service'
import { AuditService } from '#services/audit_service'
import {
  createStorageValidator,
  updateStorageValidator,
  listStoragesValidator,
  browseStorageValidator,
  copyStorageValidator,
  archiveStorageValidator,
} from '#validators/storage_validator'

/**
 * Mapeamento de provider para type do banco (backward compat)
 */
const PROVIDER_TO_TYPE: Record<StorageProvider, StorageDestinationType> = {
  aws_s3: 's3',
  minio: 's3',
  cloudflare_r2: 's3',
  google_gcs: 'gcs',
  azure_blob: 'azure_blob',
  sftp: 'sftp',
  local: 'local',
}

export default class StoragesController {
  async index({ request, response }: HttpContext) {
    const query = await request.validateUsing(listStoragesValidator)

    const page = query.page || 1
    const limit = query.limit || 20

    const storagesQuery = StorageDestination.query().orderBy('name', 'asc')

    if (query.type) {
      storagesQuery.where('type', query.type)
    }

    if (query.provider) {
      storagesQuery.where('provider', query.provider)
    }

    if (query.status) {
      storagesQuery.where('status', query.status)
    }

    if (query.search) {
      const searchTerm = `%${query.search}%`
      storagesQuery.where((builder) => {
        builder.whereILike('name', searchTerm)
      })
    }

    const storages = await storagesQuery.paginate(page, limit)

    const serialized = storages.serialize()
    serialized.data = storages.all().map((s) => ({
      ...s.serialize({
        fields: ['id', 'name', 'type', 'provider', 'status', 'isDefault', 'createdAt', 'updatedAt'],
      }),
      providerLabel: s.getProviderLabel(),
    }))

    return response.ok({
      success: true,
      data: serialized,
    })
  }

  async store(ctx: HttpContext) {
    const { request, response } = ctx
    const payload = (await request.validateUsing(createStorageValidator)) as any

    const provider: StorageProvider = payload.provider
    const type = PROVIDER_TO_TYPE[provider]

    const storage = new StorageDestination()
    storage.name = payload.name
    storage.type = type
    storage.provider = provider
    storage.status = payload.status ?? 'active'
    storage.isDefault = payload.isDefault ?? false
    storage.setConfig({ type, ...(payload.config ?? {}) } as any)

    await storage.save()

    if (storage.isDefault) {
      await StorageDestination.query().whereNot('id', storage.id).update({ isDefault: false })
    }

    await AuditService.log(
      {
        action: 'connection.created',
        entityType: 'settings',
        entityId: storage.id,
        entityName: storage.name,
        description: `Armazenamento "${storage.name}" (${storage.getProviderLabel()}) criado`,
      },
      ctx
    )

    return response.created({
      success: true,
      message: 'Armazenamento criado com sucesso',
      data: {
        ...storage.serialize({
          fields: [
            'id',
            'name',
            'type',
            'provider',
            'status',
            'isDefault',
            'createdAt',
            'updatedAt',
          ],
        }),
        providerLabel: storage.getProviderLabel(),
        config: storage.getSafeConfig(),
      },
    })
  }

  async show({ params, response }: HttpContext) {
    const storage = await StorageDestination.find(params.id)

    if (!storage) {
      return response.notFound({
        success: false,
        message: 'Armazenamento não encontrado',
      })
    }

    return response.ok({
      success: true,
      data: {
        ...storage.serialize({
          fields: [
            'id',
            'name',
            'type',
            'provider',
            'status',
            'isDefault',
            'createdAt',
            'updatedAt',
          ],
        }),
        providerLabel: storage.getProviderLabel(),
        config: storage.getSafeConfig(),
      },
    })
  }

  async update(ctx: HttpContext) {
    const { params, request, response } = ctx
    const storage = await StorageDestination.find(params.id)

    if (!storage) {
      return response.notFound({
        success: false,
        message: 'Armazenamento não encontrado',
      })
    }

    const payload = (await request.validateUsing(updateStorageValidator)) as any

    if (payload.name !== undefined) storage.name = payload.name
    if (payload.status !== undefined) storage.status = payload.status
    if (payload.isDefault !== undefined) storage.isDefault = payload.isDefault

    if (payload.provider !== undefined) {
      storage.provider = payload.provider
      storage.type = PROVIDER_TO_TYPE[payload.provider as StorageProvider]
    }

    if (payload.config !== undefined) {
      const type = storage.type
      storage.setConfig({ type, ...(payload.config ?? {}) } as any)
    }

    await storage.save()

    if (storage.isDefault) {
      await StorageDestination.query().whereNot('id', storage.id).update({ isDefault: false })
    }

    await AuditService.log(
      {
        action: 'settings.updated',
        entityType: 'settings',
        entityId: storage.id,
        entityName: storage.name,
        description: `Armazenamento "${storage.name}" atualizado`,
      },
      ctx
    )

    return response.ok({
      success: true,
      message: 'Armazenamento atualizado com sucesso',
      data: {
        ...storage.serialize({
          fields: [
            'id',
            'name',
            'type',
            'provider',
            'status',
            'isDefault',
            'createdAt',
            'updatedAt',
          ],
        }),
        providerLabel: storage.getProviderLabel(),
        config: storage.getSafeConfig(),
      },
    })
  }

  async destroy(ctx: HttpContext) {
    const { params, response } = ctx
    const storage = await StorageDestination.query()
      .where('id', params.id)
      .withCount('backups')
      .withCount('connections')
      .first()

    if (!storage) {
      return response.notFound({
        success: false,
        message: 'Armazenamento não encontrado',
      })
    }

    const backupsCount = Number(storage.$extras.backups_count ?? 0)
    const connectionsCount = Number(storage.$extras.connections_count ?? 0)

    if (backupsCount > 0 || connectionsCount > 0) {
      return response.unprocessableEntity({
        success: false,
        message: `Não é possível remover: existem ${backupsCount} backup(s) e ${connectionsCount} conexão(ões) vinculadas a este armazenamento`,
      })
    }

    const storageName = storage.name
    await storage.delete()

    await AuditService.log(
      {
        action: 'connection.deleted',
        entityType: 'settings',
        entityId: params.id,
        entityName: storageName,
        description: `Armazenamento "${storageName}" removido`,
      },
      ctx
    )

    return response.ok({
      success: true,
      message: 'Armazenamento removido com sucesso',
    })
  }

  async test({ params, response }: HttpContext) {
    const storage = await StorageDestination.find(params.id)

    if (!storage) {
      return response.notFound({
        success: false,
        message: 'Armazenamento não encontrado',
      })
    }

    try {
      await BucketExplorerService.testConnection(storage)

      return response.ok({
        success: true,
        message: 'Conexão testada com sucesso',
      })
    } catch (err: any) {
      return response.unprocessableEntity({
        success: false,
        message: `Falha no teste de conexão: ${err.message}`,
      })
    }
  }

  async browse({ params, request, response }: HttpContext) {
    const storage = await StorageDestination.find(params.id)

    if (!storage) {
      return response.notFound({
        success: false,
        message: 'Armazenamento não encontrado',
      })
    }

    const query = await request.validateUsing(browseStorageValidator)

    try {
      const result = await BucketExplorerService.listObjects(storage, query.path ?? '', {
        cursor: query.cursor,
        limit: query.limit ?? 100,
        prefix: query.prefix,
      })

      return response.ok({
        success: true,
        data: result,
      })
    } catch (err: any) {
      return response.unprocessableEntity({
        success: false,
        message: `Erro ao explorar armazenamento: ${err.message}`,
      })
    }
  }

  async startCopy(ctx: HttpContext) {
    const { params, request, response } = ctx
    const storage = await StorageDestination.find(params.id)

    if (!storage) {
      return response.notFound({
        success: false,
        message: 'Armazenamento de origem não encontrado',
      })
    }

    const payload = await request.validateUsing(copyStorageValidator)

    const destination = await StorageDestination.find(payload.destinationId)

    if (!destination) {
      return response.notFound({
        success: false,
        message: 'Armazenamento de destino não encontrado',
      })
    }

    if (storage.id === destination.id) {
      return response.unprocessableEntity({
        success: false,
        message: 'Origem e destino não podem ser o mesmo armazenamento',
      })
    }

    const job = await BucketCopyService.startCopy(storage, destination, {
      sourcePath: payload.sourcePath,
      destinationPath: payload.destinationPath,
      dryRun: payload.dryRun,
      deleteExtraneous: payload.deleteExtraneous,
    })

    await AuditService.log(
      {
        action: 'settings.updated',
        entityType: 'settings',
        entityId: storage.id,
        entityName: storage.name,
        description: `Cópia iniciada de "${storage.name}" para "${destination.name}"`,
        details: {
          metadata: {
            jobId: job.id,
            sourceId: storage.id,
            destinationId: destination.id,
            dryRun: payload.dryRun ?? false,
          },
        },
      },
      ctx
    )

    return response.accepted({
      success: true,
      message: 'Job de cópia iniciado',
      data: job,
    })
  }

  async copyStatus({ params, response }: HttpContext) {
    const job = BucketCopyService.getJob(params.jobId)

    if (!job) {
      return response.notFound({
        success: false,
        message: 'Job de cópia não encontrado',
      })
    }

    return response.ok({
      success: true,
      data: job,
    })
  }

  async startArchive(ctx: HttpContext) {
    const { params, request, response } = ctx
    const storage = await StorageDestination.find(params.id)

    if (!storage) {
      return response.notFound({
        success: false,
        message: 'Armazenamento não encontrado',
      })
    }

    const payload = await request.validateUsing(archiveStorageValidator)

    const job = await BucketArchiveService.startArchive(storage, payload.path ?? null)

    await AuditService.log(
      {
        action: 'settings.updated',
        entityType: 'settings',
        entityId: storage.id,
        entityName: storage.name,
        description: `Archive iniciado para "${storage.name}"`,
        details: {
          metadata: {
            jobId: job.id,
            path: payload.path ?? '/',
          },
        },
      },
      ctx
    )

    return response.accepted({
      success: true,
      message: 'Job de archive iniciado',
      data: job,
    })
  }

  async downloadArchive({ params, response }: HttpContext) {
    const job = BucketArchiveService.getJob(params.jobId)

    if (!job) {
      return response.notFound({
        success: false,
        message: 'Job de archive não encontrado',
      })
    }

    if (job.status !== 'ready') {
      const statusMessages: Record<string, string> = {
        pending: 'Archive ainda não foi iniciado',
        building: 'Archive está sendo gerado',
        expired: 'Archive expirou (limite de 15 minutos)',
        failed: `Archive falhou: ${job.error}`,
      }

      return response.unprocessableEntity({
        success: false,
        message: statusMessages[job.status] ?? 'Archive não está disponível',
      })
    }

    const stream = BucketArchiveService.getDownloadStream(params.jobId)

    if (!stream) {
      return response.unprocessableEntity({
        success: false,
        message: 'Stream de download não disponível',
      })
    }

    const filename = `storage-archive-${job.storageId}-${Date.now()}.tar.gz`

    response.header('Content-Type', 'application/gzip')
    response.header('Content-Disposition', `attachment; filename="${filename}"`)
    response.header('Transfer-Encoding', 'chunked')

    return response.stream(stream)
  }
}
