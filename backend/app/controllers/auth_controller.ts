import app from '@adonisjs/core/services/app'
import type { HttpContext } from '@adonisjs/core/http'
import env from '#start/env'
import { timingSafeEqual } from 'node:crypto'
import User from '#models/user'
import { registerValidator, loginValidator } from '#validators/auth'

export default class AuthController {
  private hasValidBootstrapToken(candidate: string | undefined) {
    const configuredToken = env.get('INITIAL_ADMIN_BOOTSTRAP_TOKEN')

    if (!configuredToken) {
      return !app.inProduction
    }

    if (!candidate) {
      return false
    }

    const expected = Buffer.from(configuredToken)
    const received = Buffer.from(candidate)

    if (expected.length !== received.length) {
      return false
    }

    return timingSafeEqual(expected, received)
  }

  /**
   * Registro de novo usuário
   */
  async register({ request, response }: HttpContext) {
    const payload = await request.validateUsing(registerValidator)
    const { bootstrapToken, ...userPayload } = payload

    // Verifica se existem usuários cadastrados
    const usersCount = await User.query().count('* as total').first()
    const totalUsers = usersCount ? Number(usersCount.$extras.total) : 0
    const isFirstUser = totalUsers === 0

    if (isFirstUser && !this.hasValidBootstrapToken(bootstrapToken)) {
      return response.forbidden({
        success: false,
        message: app.inProduction
          ? 'Token de bootstrap invalido ou ausente para criar o administrador inicial.'
          : 'Nao foi possivel validar o bootstrap inicial.',
      })
    }

    const isActive = isFirstUser
    const isAdmin = isFirstUser

    const user = await User.create({
      ...userPayload,
      isActive,
      isAdmin,
    })

    if (!isActive) {
      return response.created({
        success: true,
        message: 'Cadastro realizado. Aguarde aprovação de um administrador.',
      })
    }

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
          isActive: user.isActive,
          isAdmin: user.isAdmin,
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

    if (!user.isActive) {
      return response.unauthorized({
        success: false,
        message: 'Sua conta aguarda aprovação.',
      })
    }

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
          isActive: user.isActive,
          isAdmin: user.isAdmin,
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
        isActive: user.isActive,
        isAdmin: user.isAdmin,
        createdAt: user.createdAt,
      },
    })
  }

  /**
   * Verifica o status do sistema (ex: se existe usuário registrado)
   */
  async checkStatus({ response }: HttpContext) {
    const usersCount = await User.query().count('* as total').first()
    const totalUsers = usersCount ? Number(usersCount.$extras.total) : 0

    return response.ok({
      success: true,
      data: {
        hasUsers: totalUsers > 0,
        requiresBootstrapToken: totalUsers === 0 && app.inProduction,
      },
    })
  }
}
