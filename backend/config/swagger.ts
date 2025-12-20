import path from 'node:path'
import url from 'node:url'

export default {
  // Caminho raiz do projeto
  path: path.dirname(url.fileURLToPath(import.meta.url)) + '/../',
  title: 'DB Backup Manager API',
  version: '1.0.0',
  description: 'API para gerenciamento de backups de múltiplos bancos de dados',
  tagIndex: 2,
  info: {
    title: 'DB Backup Manager API',
    version: '1.0.0',
    description: 'API para gerenciamento de backups de múltiplos bancos de dados',
  },
  snakeCase: true,
  debug: false,
  ignore: ['/swagger', '/docs', '/health'],
  preferredPutPatch: 'PUT', // Prefere PUT se ambos existirem
  common: {
    parameters: {
      page: [
        'page',
        'query',
        false,
        'int',
        'Número da página para paginação',
      ],
      limit: [
        'limit',
        'query',
        false,
        'int',
        'Quantidade de itens por página',
      ],
    },
    headers: {},
  },
  securitySchemes: {
    BearerAuth: {
      type: 'http',
      scheme: 'bearer',
      bearerFormat: 'JWT',
    },
  },
  authMiddlewares: ['auth', 'auth:api'],
  defaultSecurityScheme: 'BearerAuth',
  persistAuthorization: true,
  showFullPath: false,
}
