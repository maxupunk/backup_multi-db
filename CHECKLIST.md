# 📋 Checklist do Projeto - DB Backup Manager

## 📌 Informações do Projeto

- **Nome:** DB Backup Manager
- **Tipo:** Self-Hosted, Open Source
- **Stack:** AdonisJS v6 (Backend) + Vue 3 + Vuetify (Frontend)
- **Licença:** MIT (a definir)

---

## 🚀 FASE 1 - MVP (Minimum Viable Product)

### 1. Inicialização e Estrutura do Projeto

- [x] Criar checklist do projeto
- [x] Inicializar projeto AdonisJS v6 (backend)
- [x] Inicializar projeto Vue 3 + Vite + Vuetify (frontend)
- [x] Configurar TypeScript em ambos os projetos
- [x] Configurar AdonisJS para servir arquivos estáticos do Vue (após build)
- [x] Configurar variáveis de ambiente (.env)
- [x] Configurar proxy de desenvolvimento (Vite → AdonisJS API)

### 2. Banco de Dados e Migrations

- [x] Configurar SQLite via Lucid ORM
- [x] Criar migration `connections` (credenciais dos bancos)
  - [x] Campos: id, name, type, host, port, username, password (encrypted), database, created_at, updated_at
  - [x] Implementar criptografia AES-256-GCM para senhas
- [x] Criar migration `backups` (logs e metadados)
  - [x] Campos: id, connection_id, status, file_path, file_size, retention_type, started_at, finished_at, error_message, created_at, updated_at
- [x] Criar Models Lucid (Connection, Backup)
- [x] Implementar serviço de criptografia/descriptografia

### 3. API REST - Conexões (CRUD)

- [x] POST /api/connections - Criar conexão
- [x] GET /api/connections - Listar conexões
- [x] GET /api/connections/:id - Obter conexão específica
- [x] PUT /api/connections/:id - Atualizar conexão
- [x] DELETE /api/connections/:id - Deletar conexão
- [x] POST /api/connections/:id/test - Testar conexão (Ping)

### 4. API REST - Backups

- [x] POST /api/connections/:id/backup - Iniciar backup manual
- [x] GET /api/backups - Listar todos os backups
- [x] GET /api/connections/:id/backups - Listar backups de uma conexão
- [x] GET /api/backups/:id/download - Download do arquivo de backup
- [x] DELETE /api/backups/:id - Deletar backup

### 5. Engine de Backup

- [x] Criar serviço `BackupService`
- [x] Implementar executor para `mysqldump` (MySQL/MariaDB)
- [x] Implementar executor para `pg_dump` (PostgreSQL)
- [x] Implementar streaming de output para arquivo
- [x] Configurar diretório de armazenamento de backups
- [x] Implementar compressão (gzip)

### 6. Scheduler (Agendamento)

- [x] Configurar AdonisJS Scheduler
- [x] Implementar agendamento dinâmico por conexão (1h, 6h, 12h, 24h)
- [x] Criar jobs de backup agendados
- [x] Implementar logs de execução

### 7. Lógica de Retenção (GFS Modificado)

- [x] Criar serviço `RetentionService` (completo)
- [x] Implementar lógica de pruning:
  - [x] Durante o dia: manter baseado na frequência
  - [x] Fim do dia: manter último backup do dia
  - [x] Fim da semana: manter último backup da semana
  - [x] Fim do mês: manter último backup do mês
  - [x] Fim do ano: manter último backup do ano
- [x] Criar job de limpeza automática
- [x] Implementar marcação de backups protegidos

### 8. Frontend - Dashboard

- [x] Criar layout base (Vuetify)
- [x] Implementar tema dark/light
- [x] Criar página Dashboard
  - [x] Card de resumo (total conexões, backups hoje, espaço usado)
  - [x] Lista de conexões com status do último backup
  - [x] Indicadores visuais (sucesso/erro/pendente)
- [ ] Implementar auto-refresh

### 9. Frontend - Gerenciamento de Conexões

- [x] Criar página de listagem de conexões
- [x] Criar formulário de criação/edição de conexão
- [x] Implementar validação de formulário
- [x] Implementar botão de teste de conexão
- [x] Criar modal de confirmação para exclusão

### 10. Frontend - Histórico de Backups

- [x] Criar página de histórico de backups
- [x] Implementar filtros (por conexão, por status, por data)
- [x] Implementar paginação
- [x] Adicionar botão de download
- [x] Mostrar tempo de execução e tamanho do arquivo

### 11. PWA (Progressive Web App)

- [x] Configurar manifest.json
- [x] Criar service worker básico
- [x] Adicionar ícones em múltiplos tamanhos
- [x] Configurar offline fallback

### 12. Segurança e Validação

- [x] Implementar validação de inputs (VineJS)
- [x] Configurar CORS
- [x] Implementar rate limiting
- [x] Sanitizar dados de entrada
- [x] Logs de auditoria

### 13. Documentação

- [x] Documentar API (OpenAPI/Swagger)
- [ ] Guia de instalação
- [ ] Guia de contribuição (CONTRIBUTING.md)

---

## 🔮 FASE 2 - Melhorias Futuras (Backlog)

### Autenticação e Multi-usuário

- [x] Sistema de login/registro
- [x] Gerenciamento de usuários
- [ ] Permissões por conexão

### Docker Manager

- [x] Gerenciamento de containers (listar, start, stop, restart)
- [x] Visualização de logs de containers
- [x] Gerenciamento de volumes (listar, inspecionar, remover)
- [x] Gerenciamento de networks (listar, inspecionar)
- [x] Gerenciamento de imagens (listar, inspecionar, remover, prune)
- [x] Agrupamento de containers por projeto docker-compose
- [x] Interface web com navegação dedicada (Docker Manager submenu)
- [x] Indicador de disponibilidade do socket Docker no drawer de navegação

### Destinos de Armazenamento

- [x] Local filesystem
- [x] Suporte a S3/MinIO
- [x] Suporte a Google Cloud Storage
- [x] Suporte a Azure Blob Storage
- [x] Suporte a SFTP

### Notificações

- [ ] Notificações por email
- [ ] Webhooks
- [ ] Integração com Slack/Discord

### Novos Bancos de Dados

- [ ] MongoDB
- [ ] SQLite
- [ ] SQL Server

### DevOps

- [ ] Dockerfile
- [ ] docker-compose.yml
- [ ] GitHub Actions (CI/CD)

---

## 📝 Notas de Desenvolvimento

### Convenções

- **Commits:** Conventional Commits (feat:, fix:, docs:, etc.)
- **Branch:** main (produção), develop (desenvolvimento)
- **Código:** ESLint + Prettier

### Estrutura de Pastas Atual

```
backup_multi-db/
├── backend/                     # AdonisJS v6
│   ├── app/
│   │   ├── controllers/
│   │   │   ├── backups_controller.ts    ✅ Criado
│   │   │   └── connections_controller.ts ✅ Criado
│   │   ├── exceptions/
│   │   ├── middleware/
│   │   ├── models/
│   │   │   ├── backup.ts        ✅ Criado
│   │   │   ├── connection.ts    ✅ Criado
│   │   │   └── user.ts          (padrão AdonisJS)
│   │   ├── services/
│   │   │   ├── backup_service.ts       ✅ Criado
│   │   │   └── encryption_service.ts   ✅ Criado
│   │   └── validators/
│   │       └── connection_validator.ts ✅ Criado
│   ├── config/
│   │   ├── app.ts
│   │   ├── auth.ts
│   │   ├── bodyparser.ts
│   │   ├── cors.ts
│   │   ├── database.ts          ✅ SQLite configurado
│   │   ├── hash.ts
│   │   ├── logger.ts
│   │   └── static.ts            ✅ Configurado
│   ├── database/
│   │   └── migrations/
│   │       ├── 1_create_connections_table.ts  ✅ Criado
│   │       └── 2_create_backups_table.ts      ✅ Criado
│   ├── public/                  # Vue build output ✅
│   │   ├── assets/
│   │   ├── favicon.ico
│   │   └── index.html
│   ├── start/
│   │   ├── env.ts               ✅ Configurado
│   │   ├── kernel.ts
│   │   └── routes.ts            ✅ API routes + SPA fallback
│   ├── storage/
│   │   ├── backups/             ✅ Criado
│   │   └── database/            ✅ Criado
│   ├── .env.example             ✅ Atualizado
│   └── package.json
├── frontend/                    # Vue 3 + Vuetify
│   ├── src/
│   │   ├── components/
│   │   ├── layouts/
│   │   │   └── default.vue      ✅ Navigation drawer + theme
│   │   ├── pages/
│   │   │   ├── index.vue        ✅ Dashboard
│   │   │   ├── connections/
│   │   │   │   ├── index.vue    ✅ Listagem
│   │   │   │   └── [id].vue     ✅ Form create/edit
│   │   │   ├── backups/
│   │   │   │   └── index.vue    ✅ Histórico
│   │   │   └── settings/
│   │   │       └── index.vue    ✅ Configurações
│   │   ├── services/
│   │   │   └── api.ts           ✅ Cliente HTTP
│   │   ├── types/
│   │   │   └── api.ts           ✅ Tipos TypeScript
│   │   ├── stores/
│   │   └── plugins/
│   │       └── vuetify.ts       ✅ Tema customizado
│   ├── public/
│   ├── vite.config.mts          ✅ Proxy + build configurado
│   └── package.json
├── CHECKLIST.md                 ✅ Este arquivo
├── README.md                    ✅ Documentação
└── LICENSE                      # (a criar)
```

### Comandos Úteis

```bash
# Backend
cd backend
npm run dev          # Iniciar em modo desenvolvimento (porta 3333)
npm run build        # Build para produção
npm run typecheck    # Verificar tipos TypeScript
node ace migration:run        # Executar migrations
node ace migration:rollback   # Reverter última migration

# Frontend
cd frontend
npm run dev          # Iniciar em modo desenvolvimento (porta 3000)
npm run build        # Build para produção (output: ../backend/public)
npm run type-check   # Verificar tipos TypeScript
npm run lint         # Verificar lint

# Desenvolvimento Simultâneo
# Terminal 1: cd backend && npm run dev     (porta 3333)
# Terminal 2: cd frontend && npm run dev    (porta 3000 - com proxy para API)

# Produção (após build do frontend)
cd backend
npm run dev          # ou: node bin/server.js
# Acesse: http://localhost:3333
```

### Variáveis de Ambiente (.env)

```env
# Gerar APP_KEY:
node ace generate:key

# Gerar DB_ENCRYPTION_KEY (64 caracteres hex = 32 bytes):
node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"
```

### Endpoints da API

| Método | Endpoint                      | Descrição                 |
| ------ | ----------------------------- | ------------------------- |
| GET    | `/api/health`                 | Health check              |
| GET    | `/api/stats`                  | Estatísticas do dashboard |
| GET    | `/api/connections`            | Listar conexões           |
| POST   | `/api/connections`            | Criar conexão             |
| GET    | `/api/connections/:id`        | Obter conexão             |
| PUT    | `/api/connections/:id`        | Atualizar conexão         |
| DELETE | `/api/connections/:id`        | Remover conexão           |
| POST   | `/api/connections/:id/test`   | Testar conexão            |
| POST   | `/api/connections/:id/backup` | Executar backup           |
| GET    | `/api/backups`                | Listar backups            |
| GET    | `/api/backups/:id`            | Detalhes do backup        |
| GET    | `/api/backups/:id/download`   | Download do backup        |
| DELETE | `/api/backups/:id`            | Remover backup            |
| GET    | `/api/audit-logs`             | Listar logs de auditoria  |
| GET    | `/api/audit-logs/stats`       | Estatísticas de auditoria |
| GET    | `/api/audit-logs/:id`         | Detalhes do log           |

---

**Última atualização:** 2025-12-20 12:23
**Progresso:** Fase 1 - ~85% concluída ✅

- ✅ Estrutura do projeto
- ✅ Banco de dados e migrations
- ✅ API REST completa
- ✅ Engine de backup
- ✅ **Scheduler (agendamento) - COMPLETO**
- ✅ **Lógica de retenção GFS - COMPLETO**
- ✅ Frontend Dashboard, Conexões e Backups
- ✅ PWA (manifest, service workers)
- ✅ Rate limiting
- ⏳ Documentação completa da API

```

```
