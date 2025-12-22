import type { HttpContext } from '@adonisjs/core/http'
import StorageDestination from '#models/storage_destination'
import {
  createStorageDestinationValidator,
  listStorageDestinationsValidator,
  updateStorageDestinationValidator,
} from '#validators/storage_destination_validator'

export default class StorageDestinationsController {
  async index({ request, response }: HttpContext) {
    const query = await request.validateUsing(listStorageDestinationsValidator)

    const page = query.page || 1
    const limit = query.limit || 20

    const destinationsQuery = StorageDestination.query().orderBy('name', 'asc')

    if (query.type) {
      destinationsQuery.where('type', query.type)
    }

    if (query.status) {
      destinationsQuery.where('status', query.status)
    }

    if (query.search) {
      const searchTerm = `%${query.search}%`
      destinationsQuery.where((builder) => {
        builder.whereILike('name', searchTerm)
      })
    }

    const destinations = await destinationsQuery.paginate(page, limit)

    return response.ok({
      success: true,
      data: destinations.serialize({
        fields: ['id', 'name', 'type', 'status', 'isDefault', 'createdAt', 'updatedAt'],
      }),
    })
  }

  async store({ request, response }: HttpContext) {
    const payload = (await request.validateUsing(createStorageDestinationValidator)) as any

    const destination = new StorageDestination()
    destination.name = payload.name
    destination.type = payload.type
    destination.status = payload.status ?? 'active'
    destination.isDefault = payload.isDefault ?? false
    destination.setConfig({ type: payload.type, ...(payload.config ?? {}) } as any)

    await destination.save()

    if (destination.isDefault) {
      await StorageDestination.query().whereNot('id', destination.id).update({ isDefault: false })
    }

    return response.created({
      success: true,
      message: 'Destino de armazenamento criado com sucesso',
      data: {
        ...destination.serialize({
          fields: ['id', 'name', 'type', 'status', 'isDefault', 'createdAt', 'updatedAt'],
        }),
        config: destination.getSafeConfig(),
      },
    })
  }

  async show({ params, response }: HttpContext) {
    const destination = await StorageDestination.find(params.id)

    if (!destination) {
      return response.notFound({
        success: false,
        message: 'Destino de armazenamento não encontrado',
      })
    }

    return response.ok({
      success: true,
      data: {
        ...destination.serialize({
          fields: ['id', 'name', 'type', 'status', 'isDefault', 'createdAt', 'updatedAt'],
        }),
        config: destination.getSafeConfig(),
      },
    })
  }

  async update({ params, request, response }: HttpContext) {
    const destination = await StorageDestination.find(params.id)

    if (!destination) {
      return response.notFound({
        success: false,
        message: 'Destino de armazenamento não encontrado',
      })
    }

    const payload = (await request.validateUsing(updateStorageDestinationValidator)) as any

    if (payload.name !== undefined) destination.name = payload.name
    if (payload.status !== undefined) destination.status = payload.status
    if (payload.isDefault !== undefined) destination.isDefault = payload.isDefault

    if (payload.type !== undefined) {
      destination.type = payload.type
    }

    if (payload.config !== undefined) {
      const typeToUse = payload.type ?? destination.type
      destination.setConfig({ type: typeToUse, ...(payload.config ?? {}) } as any)
    }

    await destination.save()

    if (destination.isDefault) {
      await StorageDestination.query().whereNot('id', destination.id).update({ isDefault: false })
    }

    return response.ok({
      success: true,
      message: 'Destino de armazenamento atualizado com sucesso',
      data: {
        ...destination.serialize({
          fields: ['id', 'name', 'type', 'status', 'isDefault', 'createdAt', 'updatedAt'],
        }),
        config: destination.getSafeConfig(),
      },
    })
  }

  async destroy({ params, response }: HttpContext) {
    const destination = await StorageDestination.find(params.id)

    if (!destination) {
      return response.notFound({
        success: false,
        message: 'Destino de armazenamento não encontrado',
      })
    }

    await destination.delete()

    return response.ok({
      success: true,
      message: 'Destino de armazenamento removido com sucesso',
    })
  }
}
