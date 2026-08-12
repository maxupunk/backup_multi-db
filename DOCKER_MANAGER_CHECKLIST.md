# Checklist — Docker Manager

> Revisado em 2026-08-12. O recurso é implementado no backend Loco e no
> frontend Vue; as referências à estrutura antiga foram removidas.

## Backend

- [x] Consulta do estado e do ambiente Docker.
- [x] Listagem, inspeção, início, parada, reinício e remoção de containers.
- [x] Leitura e limpeza de logs de containers.
- [x] Listagem, inspeção, exportação, backup e remoção de volumes.
- [x] Listagem, criação, inspeção, conexão e desconexão de networks.
- [x] Listagem, inspeção, remoção e prune de imagens.
- [x] Diagnósticos assíncronos e consulta do resultado.
- [x] Métricas de containers e eventos SSE.
- [x] Rotas protegidas e cobertura direta de request para todas as operações.

## Frontend

- [x] Navegação e páginas para containers, volumes, networks e imagens.
- [x] Tela de detalhes do container, logs e ações operacionais.
- [x] Exportação e backup de volume para download ou storage configurado.
- [x] Visualização de métricas e histórico de recursos.
- [x] Estados de indisponibilidade da Docker Engine.

## Validação operacional

- [x] Testes unitários e de request não dependem de uma Docker Engine ativa.
- [x] Testes de integração que criam recursos reais detectam a Engine e são
  ignorados de forma explícita quando ela não está disponível.
- [x] Operações mutáveis recebem o limitador `strict`.
