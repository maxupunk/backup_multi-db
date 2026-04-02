import type { HttpContext } from '@adonisjs/core/http'
import User from '#models/user'

export default class UsersController {
  private ensureAdmin({ auth, response }: HttpContext) {
    const currentUser = auth.user!

    if (currentUser.isAdmin) {
      return currentUser
    }

    response.forbidden({
      success: false,
      message: 'Apenas administradores podem gerenciar usuarios.',
    })

    return null
  }

  /**
   * Listar usuários com paginação e filtros
   */
  async index(ctx: HttpContext) {
    if (!this.ensureAdmin(ctx)) {
      return
    }

    const { request, response } = ctx
    const page = request.input('page', 1)
    const limit = request.input('limit', 10)
    // Optional filter by status if needed: ?active=false
    const active = request.input('active')

    const query = User.query().orderBy('createdAt', 'desc')

    if (active !== undefined && active !== null) {
      // Convert 'true'/'false' string to boolean
      const isActive = active === 'true' || active === true
      query.where('isActive', isActive)
    }

    const users = await query.paginate(page, limit)

    return response.ok(users)
  }

  /**
   * Alternar status ativo/inativo
   */
  async toggleStatus(ctx: HttpContext) {
    const currentUser = this.ensureAdmin(ctx)
    if (!currentUser) {
      return
    }

    const { params, response } = ctx
    const userToUpdate = await User.findOrFail(params.id)

    // Prevent user from changing their own status (optional safety)
    if (userToUpdate.id === currentUser.id) {
      return response.badRequest({
        success: false,
        message: 'Você não pode alterar seu próprio status.',
      })
    }

    userToUpdate.isActive = !userToUpdate.isActive
    await userToUpdate.save()

    return response.ok({
      success: true,
      message: `Usuário ${userToUpdate.isActive ? 'ativado' : 'desativado'} com sucesso.`,
      data: {
        id: userToUpdate.id,
        isActive: userToUpdate.isActive,
      },
    })
  }
}
