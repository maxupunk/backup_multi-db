/**
 * PM2 Ecosystem Configuration
 * DB Backup Manager - Backend
 *
 * @see https://pm2.keymetrics.io/docs/usage/application-declaration/
 */
module.exports = {
  apps: [
    {
      name: 'backup-manager',
      script: 'bin/server.js',
      instances: 1, // Single instance devido ao SQLite (não suporta múltiplas conexões de escrita)
      exec_mode: 'fork', // Fork mode para SQLite
      autorestart: true,
      watch: false,
      max_memory_restart: '512M',
      min_uptime: '10s',
      max_restarts: 10,

      // Configurações de ambiente
      env: {
        NODE_ENV: 'development',
        PORT: 3333,
        HOST: '0.0.0.0',
      },
      env_production: {
        NODE_ENV: 'production',
        PORT: 3333,
        HOST: '0.0.0.0',
      },

      // Logs
      log_date_format: 'YYYY-MM-DD HH:mm:ss Z',
      error_file: './logs/pm2-error.log',
      out_file: './logs/pm2-out.log',
      merge_logs: true,
      log_type: 'json',

      // Graceful shutdown
      kill_timeout: 5000,
      wait_ready: true,
      listen_timeout: 10000,

      // Health check via HTTP
      // PM2 enviará request para verificar se a aplicação está pronta
    },
  ],

  // Deploy configuration (opcional - para deploy remoto)
  deploy: {
    production: {
      user: 'node',
      host: 'localhost',
      ref: 'origin/main',
      repo: 'git@github.com:user/repo.git',
      path: '/var/www/backup-manager',
      'pre-deploy-local': '',
      'post-deploy': 'npm install && npm run build && pm2 reload ecosystem.config.cjs --env production',
      'pre-setup': '',
    },
  },
}
