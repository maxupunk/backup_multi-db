import type { HttpContext } from '@adonisjs/core/http'
import User from '#models/user'
import { registerValidator, loginValidator } from '#validators/auth'

export default class AuthController {
  /**
   * Registro de novo usuário
   */
  async register({ request, response }: HttpContext) {
    const payload = await request.validateUsing(registerValidator)

    const user = await User.create(payload)

    const token = await User.accessTokens.create(user)

    return response.created({
      success: true,
      data: {
        type: 'bearer',
        token: token.value!.release(),
        user: {
          id: user.id,
          email: user.email,
          fullName: user.fullName,
        },
      },
    })
  }

  /**
   * Login de usuário
   */
  async login({ request, response }: HttpContext) {
    const { email, password } = await request.validateUsing(loginValidator)

    const user = await User.verifyCredentials(email, password)
    const token = await User.accessTokens.create(user)

    return response.ok({
      success: true,
      data: {
        type: 'bearer',
        token: token.value!.release(),
        user: {
          id: user.id,
          email: user.email,
          fullName: user.fullName,
        },
      },
    })
  }

  /**
   * Logout (revogar token)
   */
  async logout({ auth, response }: HttpContext) {
    const user = auth.user!
    await User.accessTokens.delete(user, user.currentAccessToken.identifier)

    return response.ok({
      success: true,
      message: 'Logged out successfully',
    })
  }

  /**
   * Obter usuário autenticado
   */
  async me({ auth, response }: HttpContext) {
    const user = auth.user!

    return response.ok({
      success: true,
      data: {
        id: user.id,
        email: user.email,
        fullName: user.fullName,
        createdAt: user.createdAt,
      },
    })
  }
}