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
const StoragesController = () => import('#controllers/storages_controller')
const SystemController = () => import('#controllers/system_controller')
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
    router
      .post('/auth/register', [AuthController, 'register'])
      .use(middleware.rateLimit({ limiter: 'auth', keyBy: 'ip-email' }))
    router
      .post('/auth/login', [AuthController, 'login'])
      .use(middleware.rateLimit({ limiter: 'auth', keyBy: 'ip-email' }))

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

        router.get('connections/docker-hosts', [ConnectionsController, 'dockerHosts'])

        router.resource('connections', ConnectionsController).apiOnly()

        // ==================== Storage Destinations (legacy) ====================
        router.resource('storage-destinations', StorageDestinationsController).apiOnly()
        router.get('storage-destinations-space', [StorageDestinationsController, 'spaceAll'])
        router.get('storage-destinations/:id/space', [StorageDestinationsController, 'space'])

        // ==================== Storages (novo) ====================
        // Rotas sem :id devem vir antes das rotas com :id para evitar conflito
        router.get('storages', [StoragesController, 'index'])
        router.post('storages', [StoragesController, 'store'])
        router.get('storages/copy-jobs/:jobId', [StoragesController, 'copyStatus'])
        router.get('storages/archive-jobs/:jobId', [StoragesController, 'archiveJobStatus'])
        router.get('storages/archive-jobs/:jobId/download', [StoragesController, 'downloadArchive'])

        router.get('storages/:id', [StoragesController, 'show'])
        router.put('storages/:id', [StoragesController, 'update'])
        router.delete('storages/:id', [StoragesController, 'destroy'])

        router
          .post('storages/:id/test', [StoragesController, 'test'])
          .use(middleware.rateLimit({ limiter: 'strict' }))

        router.get('storages/:id/browse', [StoragesController, 'browse'])

        router
          .post('storages/:id/copy', [StoragesController, 'startCopy'])
          .use(middleware.rateLimit({ limiter: 'backup' }))

        router
          .post('storages/:id/archive', [StoragesController, 'startArchive'])
          .use(middleware.rateLimit({ limiter: 'backup' }))

        // Test connection - rateLimit estrito (10 req/min)
        router
          .post('connections/:id/test', [ConnectionsController, 'test'])
          .use(middleware.rateLimit({ limiter: 'strict' }))

        // Create database on existing connection
        router
          .post('connections/:id/create-database', [ConnectionsController, 'createDatabase'])
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
        router
          .post('backups/:id/restore', [BackupsController, 'restore'])
          .use(middleware.rateLimit({ limiter: 'strict' }))
        router.delete('backups/:id', [BackupsController, 'destroy'])

        // Importação de backup externo (multipart) — deve vir antes de :id
        router
          .post('backups/import', [BackupsController, 'import'])
          .use(middleware.rateLimit({ limiter: 'backup' }))

        router.get('/stats', [SystemController, 'stats'])
        router.get('/system/status', [SystemController, 'status'])

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
import transmit from '@adonisjs/transmit/services/main'

// Registra as rotas do Transmit (/__transmit/*) ANTES do fallback genérico (*)
// para que a rota SPA não sobrescreva a stream de eventos SSE.
transmit.registerRoutes()

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
