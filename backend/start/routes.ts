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

    // TODO: Connections CRUD
    // router.resource('connections', ConnectionsController).apiOnly()
    // router.post('connections/:id/test', [ConnectionsController, 'test'])
    // router.post('connections/:id/backup', [ConnectionsController, 'backup'])

    // TODO: Backups
    // router.get('backups', [BackupsController, 'index'])
    // router.get('connections/:connectionId/backups', [BackupsController, 'byConnection'])
    // router.get('backups/:id/download', [BackupsController, 'download'])
    // router.delete('backups/:id', [BackupsController, 'destroy'])
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

