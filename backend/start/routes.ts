import router from '@adonisjs/core/services/router'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import app from '@adonisjs/core/services/app'
import { middleware } from '#start/kernel'

// Lazy loading dos controllers
const ConnectionsController = () => import('#controllers/connections_controller')
const BackupsController = () => import('#controllers/backups_controller')
const AuditLogsController = () => import('#controllers/audit_logs_controller')
const AuthController = () => import('#controllers/auth_controller')
const UsersController = () => import('#controllers/users_controller')
const StorageDestinationsController = () => import('#controllers/storage_destinations_controller')
import AutoSwagger from 'adonis-autoswagger'
import swagger from '#config/swagger'

/*
|--------------------------------------------------------------------------
| API Routes
|--------------------------------------------------------------------------
*/
router
  .group(() => {
    // Swagger UI
    router.get('/swagger', async () => {
      return AutoSwagger.default.docs(router.toJSON(), swagger)
    })
    router.get('/docs', async () => {
      return AutoSwagger.default.ui('/api/swagger', swagger)
    })

    // Health check (sem rate limiting - exceção ao global, mas está dentro do prefixo que tem global)
    router.get('/health', async () => {
      return {
        status: 'ok',
        timestamp: new Date().toISOString(),
        version: '1.0.0',
      }
    })

    // ==================== Autenticação (Público) ====================
    router.get('/auth/status', [AuthController, 'checkStatus'])
    router.post('/auth/register', [AuthController, 'register'])
    router.post('/auth/login', [AuthController, 'login'])

    // ==================== Rotas Protegidas ====================
    router
      .group(() => {
        // Auth User
        router.get('/auth/me', [AuthController, 'me'])
        router.post('/auth/logout', [AuthController, 'logout'])

        // ==================== Connections ====================
        // Rota de descoberta de databases (deve vir antes do resource para não conflitar)
        router
          .post('connections/discover-databases', [ConnectionsController, 'discoverDatabases'])
          .use(middleware.rateLimit({ limiter: 'strict' }))

        router.resource('connections', ConnectionsController).apiOnly()

        // ==================== Storage Destinations ====================
        router.resource('storage-destinations', StorageDestinationsController).apiOnly()
        router.get('storage-destinations-space', [StorageDestinationsController, 'spaceAll'])
        router.get('storage-destinations/:id/space', [StorageDestinationsController, 'space'])

        // Test connection - rateLimit estrito (10 req/min)
        router
          .post('connections/:id/test', [ConnectionsController, 'test'])
          .use(middleware.rateLimit({ limiter: 'strict' }))

        // Backup manual - rateLimit de backup (5 req/5min)
        router
          .post('connections/:id/backup', [ConnectionsController, 'backup'])
          .use(middleware.rateLimit({ limiter: 'backup' }))

        // ==================== Backups ====================
        router.get('backups', [BackupsController, 'index'])
        router.get('connections/:connectionId/backups', [BackupsController, 'byConnection'])
        router.get('backups/:id', [BackupsController, 'show'])
        router.get('backups/:id/download', [BackupsController, 'download'])
        router.delete('backups/:id', [BackupsController, 'destroy'])

        // ==================== Dashboard Stats ====================
        router.get('/stats', async () => {
          const connectionModule = await import('#models/connection')
          const backupModule = await import('#models/backup')
          const { StorageSpaceService } = await import('#services/storage_space_service')
          const Connection = connectionModule.default
          const Backup = backupModule.default
          const { DateTime } = await import('luxon')

          const today = DateTime.now().startOf('day')

          const [totalConnections, activeConnections, totalBackups, backupsToday, recentBackups, storageSpaces] =
            await Promise.all([
              Connection.query().count('* as total').first(),
              Connection.query().where('status', 'active').count('* as total').first(),
              Backup.query().count('* as total').first(),
              Backup.query().where('createdAt', '>=', today.toSQL()).count('* as total').first(),
              Backup.query().preload('connection').orderBy('createdAt', 'desc').limit(5),
              StorageSpaceService.getAllDestinationsSpaceInfo(),
            ])

          return {
            success: true,
            data: {
              connections: {
                total: Number(totalConnections?.$extras.total ?? 0),
                active: Number(activeConnections?.$extras.total ?? 0),
              },
              backups: {
                total: Number(totalBackups?.$extras.total ?? 0),
                today: Number(backupsToday?.$extras.total ?? 0),
              },
              recentBackups: recentBackups.map((backup) => ({
                id: backup.id,
                connectionName: backup.connection?.name ?? 'N/A',
                status: backup.status,
                fileSize: backup.fileSize,
                createdAt: backup.createdAt,
              })),
              storageSpaces,
            },
          }
        })

        // ==================== Audit Logs ====================
        router.get('audit-logs', [AuditLogsController, 'index'])
        router.get('audit-logs/stats', [AuditLogsController, 'stats'])
        router.get('audit-logs/:id', [AuditLogsController, 'show'])

        // ==================== User Management ====================
        router.get('users', [UsersController, 'index'])
        router.patch('users/:id/status', [UsersController, 'toggleStatus'])
      })
      .use(middleware.auth())
  })
  .prefix('/api')
  .use(middleware.rateLimit({ limiter: 'global' }))

/*
|--------------------------------------------------------------------------
| SPA Fallback (Production)
|--------------------------------------------------------------------------
| In production, serve the Vue SPA for all non-API routes.
| This allows Vue Router to handle client-side routing.
|
*/
router.get('*', async ({ response }) => {
  const indexPath = join(app.publicPath(), 'index.html')

  if (existsSync(indexPath)) {
    const html = readFileSync(indexPath, 'utf-8')
    return response.header('Content-Type', 'text/html').send(html)
  }

  // Development mode - frontend is served by Vite
  return response.status(404).json({
    message: 'Frontend not built. Run `npm run build` in the frontend directory.',
    hint: 'In development, access the frontend via http://localhost:3000',
  })
})
