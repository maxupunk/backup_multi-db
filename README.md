# 🗄️ DB Backup Manager

Um sistema **pronto para uso**, **self-hosted** e **open source** para gerenciamento robusto de backups de bancos de dados.
Projetado com foco em **experiência do usuário (UX)** inovadora e **instalação simplificada** via Docker.

![Status](https://img.shields.io/badge/status-ativo-brightgreen)
![Docker](https://img.shields.io/badge/docker-ready-blue)
![License](https://img.shields.io/badge/license-MIT-blue)

## ✨ Destaques (Features)

- 🚀 **Instalação Instantânea** - Suba todo o ambiente em segundos com Docker Compose
- 🎨 **Interface Moderna (UX/UI)** - Design intuitivo, feedback visual rico e foco na usabilidade
- 📱 **100% Responsivo** - Gerencie seus backups do desktop ou celular (PWA Ready)
- 🔗 **Gerenciamento Centralizado** - Controle múltiplas conexões de banco de dados em um só lugar
- 🔒 **Segurança de Ponta** - Senhas criptografadas com AES-256-GCM
- ⏰ **Automação Inteligente** - Agendamentos flexíveis e retenção automática (GFS)
- 📦 **Suporte Multi-Banco** - Compatível nativamente com MySQL, MariaDB e PostgreSQL
- 🐳 **Docker Manager** - Gerencie containers, volumes, redes e imagens diretamente pela interface web

## 🏗️ Stack Tecnológica

| Camada             | Tecnologia                     |
| ------------------ | ------------------------------ |
| **Backend**        | AdonisJS v6 (TypeScript)       |
| **Frontend**       | Vue 3 + Vuetify 3 (TypeScript) |
| **Banco de Dados** | SQLite (via Lucid ORM)         |
| **Build Tool**     | Vite                           |

## 🗃️ Bancos de Dados Suportados

- ✅ MySQL
- ✅ MariaDB
- ✅ PostgreSQL

## ☁️ Destinos de Backup (Storage)

O sistema suporta nativamente o envio seguro de backups para múltiplos destinos:

- 📂 **Local** (Filesystem do servidor)
- ☁️ **AWS S3** (e compatíveis: MinIO, DigitalOcean Spaces, Backblaze B2, etc.)
- 🟦 **Azure Blob Storage**
- 🟧 **Google Cloud Storage (GCS)**
- 📁 **SFTP** (Transferência segura para servidores remotos)

## 🚀 Início Rápido (Quick Start)

Tenha o sistema rodando em menos de 2 minutos usando Docker:

```bash
# 1. Clone o repositório
git clone https://github.com/seu-usuario/db-backup-manager.git
cd db-backup-manager

# 2. Inicie com Docker Compose
docker compose up -d
```

Acesse imediatamente: **http://localhost:3000**

> O sistema já vem **configurado e pronto para uso** com um banco de dados interno e configurações padrão.

---

## 📋 Pré-requisitos (Para Instalação Manual)

- **Node.js** >= 20.x
- **npm** >= 10.x
- **mysqldump** (para MySQL/MariaDB) - incluído no MySQL Client
- **pg_dump** (para PostgreSQL) - incluído no PostgreSQL Client

## �️ Instalação Manual (Desenvolvimento)

### 1. Clone o repositório

```bash
git clone https://github.com/seu-usuario/db-backup-manager.git
cd db-backup-manager
```

### 2. Instale as dependências

```bash
# Backend
cd backend
npm install

# Frontend
cd ../frontend
npm install
```

### 3. Configure o ambiente

```bash
cd backend
cp .env.example .env

# Gerar chave da aplicação
node ace generate:key

# Gerar chave de criptografia para senhas dos bancos
# Copie o resultado para DB_ENCRYPTION_KEY no .env
node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"
```

### 4. Execute as migrations

```bash
cd backend
node ace migration:run
```

### 5. Inicie o desenvolvimento

```bash
# Terminal 1 - Backend (porta 3333)
cd backend
npm run dev

# Terminal 2 - Frontend (porta 3000)
cd frontend
npm run dev
```

Acesse: **http://localhost:3000**

## 📁 Estrutura do Projeto

```
db-backup-manager/
├── backend/              # AdonisJS v6 API
│   ├── app/
│   │   ├── controllers/
│   │   ├── models/
│   │   ├── services/
│   │   └── validators/
│   ├── config/
│   ├── database/
│   │   └── migrations/
│   ├── public/           # Build do frontend (produção)
│   └── storage/
│       ├── backups/      # Arquivos de backup
│       └── database/     # SQLite
├── frontend/             # Vue 3 + Vuetify SPA
│   └── src/
│       ├── components/
│       ├── pages/
│       ├── services/
│       └── stores/
└── CHECKLIST.md
```

## 🔐 Política de Retenção (GFS Modificado)

O sistema implementa uma política de retenção Grandfather-Father-Son modificada:

| Período       | Retenção                                 |
| ------------- | ---------------------------------------- |
| Durante o dia | Mantém 1 backup concluído por hora       |
| Fim do dia    | Último backup do dia                     |
| Fim da semana | Último backup da semana                  |
| Fim do mês    | Último backup do mês                     |
| Fim do ano    | Último backup do ano                     |

A política de retenção é gerenciada em runtime pela interface de configurações ou pela API do sistema.

## 🛠️ Scripts Disponíveis

### Backend

```bash
npm run dev        # Desenvolvimento com HMR
npm run build      # Build para produção
npm run typecheck  # Verificação de tipos
npm run lint       # ESLint
npm run test       # Testes
```

### Frontend

```bash
npm run dev        # Desenvolvimento com Vite
npm run build      # Build para produção (output: ../backend/public)
npm run lint       # ESLint
```

## 🐳 Docker Manager

O **Docker Manager** permite gerenciar toda a infraestrutura de containers diretamente pela interface web, sem precisar de acesso ao terminal.

### Funcionalidades

| Recurso | Ações disponíveis |
|---|---|
| **Containers** | Listar, start, stop, restart, ver logs, inspecionar |
| **Volumes** | Listar, inspecionar, remover |
| **Redes** | Listar, inspecionar |
| **Imagens** | Listar, inspecionar, remover, prune |

### Endpoints da API

Todas as rotas requerem autenticação (`Authorization: Bearer <token>`).

```
GET    /api/docker/containers
GET    /api/docker/containers/:id
POST   /api/docker/containers/:id/start
POST   /api/docker/containers/:id/stop
POST   /api/docker/containers/:id/restart
GET    /api/docker/containers/:id/logs

GET    /api/docker/volumes
GET    /api/docker/volumes/:name
DELETE /api/docker/volumes/:name

GET    /api/docker/networks
GET    /api/docker/networks/:id

GET    /api/docker/images
GET    /api/docker/images/:id
DELETE /api/docker/images/:id
POST   /api/docker/images/prune
```

### Requisito

O socket Docker deve estar acessível pelo container backend. No `docker-compose.yml` isso já está configurado:

```yaml
volumes:
  - /var/run/docker.sock:/var/run/docker.sock
```

Quando o socket não está disponível, as rotas de listagem retornam `{ available: false }` e a interface exibe um indicador de aviso no menu de navegação.

---

## 🐳 Docker

### Desenvolvimento

```bash
cd backend
docker compose -f docker-compose.dev.yml up --build
```

### Produção (somente Docker)

```bash
cd backend

# Build e iniciar
docker compose up --build -d

# Verificar status
docker compose ps

# Ver logs
docker compose logs -f

# Parar
docker compose down
```

### Variáveis de Ambiente (Produção)

Use o mesmo template do backend:

```bash
cd backend
cp .env.example .env

# Gere a APP_KEY
node ace generate:key

# Gere a chave de criptografia e copie para DB_ENCRYPTION_KEY no .env
node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"

# Defina um token forte para o primeiro administrador em producao
# Exemplo: node -e "console.log(require('crypto').randomBytes(24).toString('hex'))"
# Copie para INITIAL_ADMIN_BOOTSTRAP_TOKEN no .env
```

## Autenticacao inicial

- Em producao, o primeiro cadastro administrativo exige `INITIAL_ADMIN_BOOTSTRAP_TOKEN`.
- O token deve ser informado na tela de cadastro quando o sistema ainda nao possui usuarios.
- Os bearer tokens expiram conforme `AUTH_ACCESS_TOKEN_EXPIRES_IN` (padrao: `7d`).

## 📖 API Endpoints

### Conexões

| Método | Endpoint                      | Descrição         |
| ------ | ----------------------------- | ----------------- |
| GET    | `/api/connections`            | Listar conexões   |
| POST   | `/api/connections`            | Criar conexão     |
| GET    | `/api/connections/:id`        | Obter conexão     |
| PUT    | `/api/connections/:id`        | Atualizar conexão |
| DELETE | `/api/connections/:id`        | Deletar conexão   |
| POST   | `/api/connections/:id/test`   | Testar conexão    |
| POST   | `/api/connections/:id/backup` | Iniciar backup    |

### Backups

| Método | Endpoint                    | Descrição       |
| ------ | --------------------------- | --------------- |
| GET    | `/api/backups`              | Listar backups  |
| GET    | `/api/backups/:id/download` | Download backup |
| DELETE | `/api/backups/:id`          | Deletar backup  |

## 🤝 Contribuindo

Contribuições são bem-vindas! Por favor, leia o [CONTRIBUTING.md](CONTRIBUTING.md) para detalhes.

1. Fork o projeto
2. Crie sua branch (`git checkout -b feature/nova-funcionalidade`)
3. Commit suas mudanças (`git commit -m 'feat: adiciona nova funcionalidade'`)
4. Push para a branch (`git push origin feature/nova-funcionalidade`)
5. Abra um Pull Request

## 📄 Licença

Este projeto está sob a licença MIT. Veja o arquivo [LICENSE](LICENSE) para mais detalhes.

---

**Desenvolvido com ❤️ para a comunidade open source**
