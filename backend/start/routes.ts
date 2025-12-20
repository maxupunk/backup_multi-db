/*
|--------------------------------------------------------------------------
| Routes file
|--------------------------------------------------------------------------
|
| The routes file is used for defining the HTTP routes.
|
*/

import router from '@adonisjs/core/services/router'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import app from '@adonisjs/core/services/app'

// Lazy loading dos controllers
const ConnectionsController = () => import('#controllers/connections_controller')
const BackupsController = () => import('#controllers/backups_controller')

/*
|--------------------------------------------------------------------------
| API Routes
|--------------------------------------------------------------------------
*/
router
  .group(() => {
    // Health check
    router.get('/health', async () => {
      return {
        status: 'ok',
        timestamp: new Date().toISOString(),
        version: '1.0.0',
      }
    })

    // ==================== Connections ====================
    router.resource('connections', ConnectionsController).apiOnly()
    router.post('connections/:id/test', [ConnectionsController, 'test'])
    router.post('connections/:id/backup', [ConnectionsController, 'backup'])

    // ==================== Backups ====================
    router.get('backups', [BackupsController, 'index'])
    router.get('connections/:connectionId/backups', [BackupsController, 'byConnection'])
    router.get('backups/:id', [BackupsController, 'show'])
    router.get('backups/:id/download', [BackupsController, 'download'])
    router.delete('backups/:id', [BackupsController, 'destroy'])

    // ==================== Dashboard Stats ====================
    router.get('/stats', async () => {
      const Connection = (await import('#models/connection')).default
      const Backup = (await import('#models/backup')).default
      const { DateTime } = await import('luxon')

      const today = DateTime.now().startOf('day')

      const [totalConnections, activeConnections, totalBackups, backupsToday, recentBackups] =
        await Promise.all([
          Connection.query().count('* as total').first(),
          Connection.query().where('status', 'active').count('* as total').first(),
          Backup.query().count('* as total').first(),
          Backup.query().where('createdAt', '>=', today.toSQL()).count('* as total').first(),
          Backup.query()
            .preload('connection')
            .orderBy('createdAt', 'desc')
            .limit(5),
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
        },
      }
    })
  })
  .prefix('/api')

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

