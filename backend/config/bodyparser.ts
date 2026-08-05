import { defineConfig } from '@adonisjs/core/bodyparser'

const bodyParserConfig = defineConfig({
  /**
   * The bodyparser middleware will parse the request body
   * for the following HTTP methods.
   */
  allowedMethods: ['POST', 'PUT', 'PATCH', 'DELETE'],

  /**
   * Config for the "application/x-www-form-urlencoded"
   * content-type parser
   */
  form: {
    convertEmptyStringsToNull: true,
    types: ['application/x-www-form-urlencoded'],
  },

  /**
   * Config for the JSON parser
   */
  json: {
    convertEmptyStringsToNull: true,
    types: [
      'application/json',
      'application/json-patch+json',
      'application/vnd.api+json',
      'application/csp-report',
    ],
  },

  /**
   * Config for the "multipart/form-data" content-type parser.
   * File uploads are handled by the multipart parser.
   */
  multipart: {
    /**
     * Auto-processamento restrito à ÚNICA rota que aceita upload.
     *
     * Com `autoProcess: true`, qualquer rota POST/PUT/PATCH/DELETE que
     * recebesse `multipart/form-data` teria o corpo gravado no diretório
     * temporário do sistema até o limite abaixo — 500 MB por requisição, em
     * rotas que nunca deveriam receber arquivo. Se o `/tmp` do container for
     * montado como tmpfs, esse conteúdo vai direto para a RAM.
     *
     * As demais rotas continuam recebendo `request.multipart` e podem
     * processar manualmente, se algum dia precisarem.
     */
    autoProcess: ['/api/backups/import'],
    convertEmptyStringsToNull: true,
    processManually: [],

    /**
     * Maximum limit of data to parse including all files
     * and fields.
     * Aumentado para suportar importação de arquivos de backup grandes.
     * Só se aplica à rota listada em `autoProcess`.
     */
    limit: '500mb',
    types: ['multipart/form-data'],
  },
})

export default bodyParserConfig
