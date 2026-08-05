# Roadmap de Redução de Memória e Estabilidade

**Escopo:** backend AdonisJS (`backend/`), frontend Vue/Vuetify (`frontend/`), runtime Node/V8 e container.
**Data da avaliação:** 2026-08-04
**Documento anterior:** [MEMORY_ANALYSIS.md](MEMORY_ANALYSIS.md) — todos os 15 itens do plano original estão implementados. Este roadmap parte **daquele ponto** e ataca a segunda camada de problemas, que é qualitativamente diferente: a primeira rodada eliminou *churn de estado ocioso*; esta ataca **picos sob carga (backup/restore/archive) e retenção sem limite**.

---

## Status de implementação

### F0 — Instrumentação

- [x] **F0.1** — Ler limites de cgroup v1/v2 em vez de `os.totalmem()` ([container_memory_probe.ts](backend/app/services/container_memory_probe.ts)); alimenta `/api/system/status`, `/api/stats` e o evento SSE de recursos
- [x] **F0.2** — High water mark de RSS/heap + log de pressão acima de 70% ([memory_watermark_service.ts](backend/app/services/memory_watermark_service.ts)) — **sinal por log**, ver *Remoção do painel de heap*
- [x] **F0.3** — `--heapsnapshot-near-heap-limit=1` + `--diagnostic-dir` em volume; heap configurável por `NODE_MAX_OLD_SPACE_MB` ([docker-entrypoint.sh](backend/docker-entrypoint.sh))
- [x] **F0.4a** — Testes de regressão da instrumentação ([memory_instrumentation.spec.ts](backend/tests/unit/memory_instrumentation.spec.ts))
- [ ] **F0.4b** — Scripts de carga reproduzíveis (backup 5 GB, restore com filtros, archive 2 000 objetos, rclone > 30 min) — **pendente**, ver *Pendências*

### F1 — Contenção de picos

- [x] **F1.1** — Serializar downloads no archive de bucket ([bucket_archive_service.ts](backend/app/services/storage/bucket_archive_service.ts) — `appendEntry`)
- [x] **F1.2** — Backpressure no pipeline de dump via `pipeline()` ([backup_service.ts](backend/app/services/backup_service.ts) — `executeDumpProcess`)
- [x] **F1.3** — `ProcessOutputBuffer` em `BackupService` e `BucketCopyService`
- [x] **F1.4** — Fixar pool do SQLite em 1 conexão ([config/database.ts](backend/config/database.ts))
- [x] **F1.5** — Throttle no progresso do archive

**Achado extra corrigido junto:** `executeDumpProcess` lia `dumpProcess.exitCode` no evento `finish` do arquivo. Quando a escrita terminava antes do evento de saída do processo, `exitCode` ainda era `null` e **nenhum** dos dois ramos resolvia a Promise — o backup ficava pendurado para sempre. Agora a saída do processo e a conclusão do stream são aguardadas separadamente e combinadas.

**Decisões tomadas durante a implementação:**

- `cache_size` mantido em `-4096` (4 MB). Com o pool em 1 conexão o custo total já caiu de até 40 MB para 4 MB; reduzir para 2 MB traria ganho marginal com risco real na query de histórico de 15 dias. Medir antes de mexer.
- Concorrência do archive fixada em 1. O `archiver` processa uma entrada por vez de qualquer forma, então download e compressão do arquivo corrente continuam simultâneos — só não há prefetch do próximo. A/B do pool confirmou ausência de regressão de latência (suíte funcional: 8s com `max: 1` vs 8–9s com o pool default).

**Testes adicionados:** [bucket_archive_streaming.spec.ts](backend/tests/unit/bucket_archive_streaming.spec.ts) (downloads simultâneos + integridade do tar.gz), [backup_dump_pipeline.spec.ts](backend/tests/unit/backup_dump_pipeline.spec.ts) (checksum byte a byte de 4 MB, dump vazio, exit code, truncamento de stderr, binário ausente sem travar).

### F2 — Crescimento histórico

- [x] **F2.1** — Listagem do archive via async generator (`iterateFiles`), sem materializar o bucket
- [x] **F2.2** — `pipeline()` no restore em vez de `.pipe()` encadeado ([restore_service.ts](backend/app/services/restore_service.ts))
- [x] **F2.3** — Retention com projeção enxuta + UPDATE agrupado ([retention_service.ts](backend/app/services/retention_service.ts))
- [x] **F2.4** — Retenção de audit logs: 30 dias (faixa de diagnóstico 15–30), configurável ([audit_retention_service.ts](backend/app/services/audit_retention_service.ts))
- [x] **F2.5** — Memoização da config descriptografada e da chave AES

**Dois bugs encontrados pelos testes durante a implementação:**

1. **Restore podia reportar sucesso com o banco pela metade.** Ao trocar para `pipeline()`, a primeira versão classificava a falha de escrita no stdin pelo código de erro (`EPIPE`). O teste mostrou que no Windows o código é outro — a classificação por código era frágil. A regra passou a ser: **o desfecho do processo decide**. Exit code ≠ 0 → reporta o erro do banco (stderr); exit code 0 com a cadeia quebrada → falha explícita, porque o dump foi entregue incompleto.
2. **Fuso horário na projeção do retention.** O Lucid grava `@column.dateTime()` em hora **local** sem marcador de timezone. A primeira versão do `parseTimestamp` interpretava como UTC, deslocando toda data pelo offset do servidor (3h em São Paulo) — o que mudaria o bucket GFS e **apagaria os backups errados**. O teste A/B contra os modelos completos pegou isso antes de qualquer execução real.

**Decisões tomadas durante a implementação:**

- `totalFiles` do archive passa a ser preenchido só ao fim da varredura. A UI já tratava `null` (barra indeterminada + "N / ?"), então o contrato visual não mudou — e agora é honesto, porque o total realmente não é conhecido antes de percorrer todas as páginas.
- O caso "bucket vazio" deixou de ser um branch especial: com listagem incremental, o laço simplesmente não executa e o `finalize()` produz o mesmo tar.gz vazio.
- Cache da config descriptografada em `WeakMap` por instância (não campo de classe): não altera a forma do objeto serializado pelo Lucid e a entrada morre junto com o modelo — config em claro não sobrevive ao request.
- Poda de auditoria com teto de 20k registros por execução e `whereIn` em lotes de 500, respeitando o limite de variáveis do SQLite. Janela padrão de **30 dias**: o log é ferramenta de diagnóstico recente, não arquivo histórico.

**Testes adicionados:** [restore_pipeline.spec.ts](backend/tests/unit/restore_pipeline.spec.ts), [retention_projection.spec.ts](backend/tests/functional/retention_projection.spec.ts) (A/B contra os modelos Lucid), [audit_retention.spec.ts](backend/tests/functional/audit_retention.spec.ts), [storage_config_memoization.spec.ts](backend/tests/functional/storage_config_memoization.spec.ts).

### F3 — Filtros de restore

- [x] **F3.1a** — `StringDecoder` nos filtros (correção de corrupção UTF-8)
- [x] **F3.1b** — Cadeia colapsada em um único Transform + teto de linha (64 MB)

**O bug de corrupção era real e foi reproduzido antes da correção.** O teste falhou exatamente como previsto: `CREATE TABLE informação` chegava ao banco como `CREATE TABLE informa��ão`. Toda restauração com filtro ativo (`schema-only`, `data-only`, `noOwner`, `noPrivileges`, …) sobre um dump com acentuação corrompia dados silenciosamente, sempre que um caractere multibyte caísse na fronteira de um chunk de 64 KB — ou seja, com alta probabilidade em qualquer dump grande em português.

**Como a equivalência foi garantida (F3.1b):** o teste [restore_filters_equivalence.spec.ts](backend/tests/unit/restore_filters_equivalence.spec.ts) mantém a **cadeia antiga como implementação de referência** e compara as duas saídas byte a byte em toda a matriz: 3 modos × 7 combinações de opções × 5 tamanhos de chunk (1, 3, 7, 64 e 4096 bytes) para PostgreSQL, mais MySQL com/sem `noCreateDb`, mais os casos de dump sem quebra de linha final.

**Sutileza preservada de propósito:** os filtros `data-only` emitiam o resto final sem quebra de linha **sem** reavaliar a allowlist, enquanto os `schema-only` reavaliavam. Em vez de "corrigir" isso silenciosamente, a diferença virou explícita no tipo `LineFilterRules` (`keepLine` vs `keepTrailing`) e está coberta por teste. Mudar esse comportamento é uma decisão separada, não um efeito colateral de refatoração.

**Teto de linha:** 64 MB, folgado para não afetar dumps reais (`mysqldump --extended-insert` com blobs fica muito abaixo), mas suficiente para impedir que um dump numa linha só leve o processo ao OOM. O limite é injetável para permitir teste sem alocar 64 MB.

**Testes adicionados:** [restore_filters.spec.ts](backend/tests/unit/restore_filters.spec.ts) (7 testes de fronteira UTF-8 e teto de linha), [restore_filters_equivalence.spec.ts](backend/tests/unit/restore_filters_equivalence.spec.ts) (6 testes de equivalência byte a byte).

### F4 — Runtime e container

- [x] **F4.1** — Heap do V8 de 200 → ~65% do limite do container, **calculado no boot** ([docker-memory.sh](backend/docker-memory.sh)); `reservations` de 256M → 128M
- [x] **F4.2** — `autoProcess` do multipart restrito à rota `/api/backups/import`
- [x] **F4.3** — Broadcast SSE só com assinantes + polling adaptativa (10s ativo / 30s ocioso)
- [x] **F4.4** — Cache de nome e labels por container id ([docker_container_monitoring_service.ts](backend/app/services/docker_container_monitoring_service.ts))
- [x] **F4.5** — Migração do alias deprecado `desination` → `destination`

**Correção de um achado do diagnóstico (item 19).** A avaliação inicial classificou `desination` como um typo do projeto que seria "ignorado" pelo framework. **Isso estava errado.** `desination` é um alias oficialmente deprecado do `@adonisjs/logger`, resolvido em `const resolvedDestination = dest ?? desination` — a configuração sempre funcionou como esperado. A mudança feita é higiene (sair de uma chave deprecada), não correção de bug.

**Decisões tomadas durante a implementação:**

- **Polling adaptativa com disparo imediato.** Espaçar o ciclo para 30s quando ninguém observa economiza recurso, mas deixaria o painel recém-aberto em branco por até 30s. O serviço passou a ouvir o evento `subscribe` do Transmit: ao chegar um assinante, volta ao intervalo ativo **e dispara um ciclo na hora**. Sem isso, a economia de memória viraria regressão visível de UX.
- **Granularidade do histórico preservada.** O intervalo ocioso de 30s não altera os pontos gravados, porque a persistência já tinha intervalo mínimo de 60s.
- **`autoProcess` em vez de reescrever o import.** Reduzir o limite global exigiria migrar a importação para multipart manual — refatoração de um fluxo em produção. Restringir `autoProcess` à única rota de upload atinge o mesmo objetivo (nenhuma outra rota grava 500 MB em disco/tmpfs) sem tocar no fluxo.
- **Cache do `docker inspect` podado pela lista corrente:** container que some perde a entrada, então o Map não cresce conforme containers são recriados.
- **A regra dos 65% virou cálculo, não constante.** A primeira versão gravou `320` em três lugares (`.env.example`, `docker-compose.yml`, entrypoint) com a instrução de "ajustar na mesma proporção" se o limite do container mudasse. Isso é acoplamento manual que apodrece na primeira vez que alguém sobe `limits.memory` — e falha nos dois sentidos: heap alto demais para o limite dá OOM killer (exit 137), heap baixo demais dá `JavaScript heap out of memory` com o container pela metade. O entrypoint passou a ler o cgroup no boot e derivar o valor; `NODE_MAX_OLD_SPACE_MB` continua vencendo quando informado. Coberto por [docker_memory.test.sh](backend/tests/shell/docker_memory.test.sh) (16 casos: v1/v2, sentinelas de "sem limite", teto, override válido e inválido, precedência).

**Lacuna de cobertura encontrada e fechada:** a rota `/api/backups/import` — a única de upload do sistema — **não tinha nenhum teste**. A mudança de `autoProcess` poderia tê-la quebrado silenciosamente. Foram adicionados testes de upload de ponta a ponta antes de considerar a fase concluída.

**Testes adicionados:** [backup_import_upload.spec.ts](backend/tests/functional/backup_import_upload.spec.ts) (upload real, ausência de arquivo, extensão inválida e verificação do `autoProcess`).

### F5 — Frontend

- [x] **F5.1** — Histórico de heap: 20.000 → 1.500 pontos, inserção incremental, persistência 1×/min — **posteriormente removido junto com o painel**, ver *Remoção do painel de heap*
- [x] **F5.2** — **Nenhuma mudança necessária** (achado incorreto — ver abaixo)
- [x] **F5.3** — [useNotificationListener](frontend/src/composables/useNotificationListener.ts) com desregistro automático

**Correção de um achado do diagnóstico (item 17).** A avaliação afirmou que o build era "um bundle único sem `manualChunks`". **Medição desmentiu isso.** O build já sai bem fatiado:

- rotas em chunks separados por página (o `unplugin-vue-router` gera imports dinâmicos por padrão);
- componentes Vuetify em chunks próprios (`VDataTable` 38 kB, `VList` 26 kB, `VOverlay` 26 kB, `VSelect` 24 kB, …), graças ao `vite-plugin-vuetify`;
- maior chunk: **196,77 kB (73,20 kB gzip)** — o núcleo do framework, que é cacheado.

Adicionar `manualChunks` agruparia `node_modules` num vendor único e **desfaria** o fatiamento por componente que existe hoje. A mudança foi descartada com base na medição, não implementada às cegas.

**O que mudou de fato em F5.1:** a cada 10 segundos o composable reconstruía o histórico inteiro — `filter` + spread + `sort` + `slice` sobre ~17.280 objetos — e serializava alguns MB para o `localStorage`, tudo na thread principal. Além do custo, o resultado estourava a cota típica de 5 MB e a falha era engolida por um `catch` vazio. Agora:

- inserção incremental no fim do array (os pontos chegam em ordem cronológica, então não há `sort` nem varredura por duplicata a cada poll);
- compactação progressiva: a metade mais recente fica na resolução nativa de 10s (cobre com folga as visões de 1h e 6h) e a metade antiga é rareada pela metade, repetidamente, até caber em 1.500 pontos;
- gravação no `localStorage` no máximo 1×/minuto, mais uma gravação final no unmount.

**Efeito colateral corrigido durante a fase:** o payload SSE de recursos não carregava os campos `source`/`containerLimited` adicionados em F0.1, o que quebrou a tipagem nas telas que compõem `SystemStatus` a partir do evento em tempo real. O emitter passou a enviá-los — sem isso o painel não saberia dizer se está mostrando a memória do host ou o limite do container.

---

## Remoção do painel de heap

Depois da execução do roadmap, o painel "Heap do Processo" do dashboard (cartões *Heap V8*, *RSS do Processo* e *Sinais auxiliares do processo*) foi **removido a pedido**, junto com o endpoint que existia só para alimentá-lo.

**Removido:**

| Camada | Item |
| ------ | ---- |
| Frontend | `SystemHeapPanel.vue`, `useSystemHeapSnapshots.ts`, `systemApi.heap()`, tipos `SystemHeapSnapshot` e `MemoryWatermark` |
| Backend | `GET /api/system/heap`, `POST /api/system/heap/reset`, `SystemController.heap` / `resetHeapWatermark`, `SystemMonitoringService.getHeapSnapshot` e os helpers `getActiveHandlesCount` / `getActiveRequestsCount` que só ele usava |

**Mantido — e por quê:**

- **[ContainerMemoryProbe](backend/app/services/container_memory_probe.ts) (F0.1) permanece.** Ele não servia ao painel de heap: é o que faz o cartão de RAM do dashboard mostrar o limite do cgroup em vez da RAM do host. Removê-lo traria de volta o número errado dentro do container.
- **[MemoryWatermarkService](backend/app/services/memory_watermark_service.ts) (F0.2) permanece**, alimentado pelo ciclo de polling de métricas. O guardrail de pressão de memória nunca dependeu do painel — o sinal sempre saiu por log.

Com o endpoint fora, os picos acumulados ficariam sem leitor. Em vez de deixar estado acumulado que ninguém lê, `getWatermark()` (que montava um payload HTTP) deu lugar a `getPeaks()`, e **os picos passaram a compor a própria linha de `warn` de pressão** — que é o momento em que essa informação é útil: saber se o número atual é um recorde ou um platô recorrente.

**O que se perde:** a curva de RSS/heap por navegador, com janelas de 1h/6h/24h/48h. O histórico de CPU e RAM do sistema continua no dashboard, servido pela tabela de histórico do backend (`/api/system/resources/history`), que não foi tocada.

---

## Balanço da execução

**Todas as mudanças de código das fases F0–F5 foram implementadas.** Suíte do backend: **176 testes passando** (eram 123 antes de começar). `tsc --noEmit` limpo no backend e `vue-tsc` limpo no frontend.

Resta um item de infraestrutura de teste (F0.4b — scripts de carga) e a medição em ambiente real; ver *Pendências*.

### O que foi encontrado durante a execução e não estava previsto

| # | Achado | Onde apareceu |
| - | ------ | ------------- |
| 1 | **Corrupção de UTF-8 em restaurações** — reproduzida antes da correção: `informação` chegava ao banco como `informa��ão`. Atingia qualquer restore com filtro sobre dump acentuado. | F3.1a |
| 2 | **Backup podia ficar pendurado para sempre** — `executeDumpProcess` lia `exitCode` no evento `finish` do arquivo; se a escrita terminasse antes da saída do processo, nenhum ramo resolvia a Promise. | F1.2 |
| 3 | **Restore podia reportar sucesso com o banco pela metade** — primeira versão do `pipeline()` classificava a falha de stdin por código de erro; o teste mostrou que o código varia por plataforma. | F2.2 |
| 4 | **Bug de fuso na projeção do retention** — interpretar como UTC os timestamps que o Lucid grava em hora local deslocaria as datas em 3h e apagaria os backups errados. Pego pelo teste A/B antes de qualquer execução real. | F2.3 |
| 5 | **A única rota de upload do sistema não tinha teste algum.** | F4.2 |

### Achados do diagnóstico que a execução desmentiu

| # | O que o diagnóstico afirmou | O que a verificação mostrou |
| - | --------------------------- | --------------------------- |
| 17 | "Bundle único sem `manualChunks`" | O build já sai fatiado por rota e por componente Vuetify; maior chunk 196 kB (73 kB gzip). `manualChunks` pioraria. **Nenhuma mudança feita.** |
| 19 | "Typo `desination` é ignorado pelo framework" | É um alias **oficialmente deprecado** do `@adonisjs/logger`, resolvido em `dest ?? desination`. A configuração sempre funcionou. Mudança feita foi higiene, não correção. |

### Pendências

Todas as mudanças de código foram entregues. O que **não** foi feito, com o motivo:

| Item | Situação | Por quê |
| ---- | -------- | ------- |
| **F0.4b — scripts de carga** | Não criado | Precisa de banco de ~5 GB, bucket com 2 000 objetos e destino remoto real. Os scripts podem ser escritos, mas só produzem valor rodando no ambiente do cliente. |
| **Guardrail "dump pausa o stdout quando o consumidor satura"** | Sem teste | Não há assertiva determinística disso em teste unitário: com disco rápido o backpressure pode nunca engatar, e um teste que depende de timing seria instável. A garantia real vem de `pipeline()` (estrutural) e do cenário de carga. O que **está** coberto é o efeito observável: checksum e conteúdo idênticos byte a byte atravessando o pipeline completo. |
| **Guardrail "stderr do rclone truncado"** | Código feito, sem teste | Exigiria um binário `rclone` falso no PATH — frágil entre plataformas. O equivalente no dump **está** testado (2 MB de stderr truncados em 256 KB), e ambos usam o mesmo `ProcessOutputBuffer`. |
| **Cenário de carga no CI com orçamento de RSS** | Não feito | Depende de F0.4b. |
| **Reduzir `multipart.limit` global para 2 MB** | Decisão consciente de não fazer | Exigiria migrar a importação para multipart manual. `autoProcess` restrito à rota de upload atinge o mesmo objetivo sem tocar num fluxo em produção. |
| **Metas de RSS da seção 8** | Não medidas | São **alvos de projeto**. Sem o endpoint de heap (ver *Remoção do painel de heap*), confirmar por fora: `docker stats --no-stream` durante o cenário, e o `warn` de pressão no log — que já carrega `peakRssBytes` / `peakHeapUsedBytes` acumulados desde o start do processo. Reiniciar o container zera a janela. |

**Verificação que foi possível fechar agora:** o item "garantir que `/tmp` no container é disco, não tmpfs" (F4.2) está **resolvido** — não há nenhuma declaração `tmpfs` no `Dockerfile`, no `docker-compose.yml` nem no `docker-compose.dev.yml`, então `/tmp` é a camada de disco do container. Uploads nunca vão para a RAM por esse caminho.

**Guardrail "falha de restore não deixa handles ativos":** implementado de forma mais direta do que o proposto. Em vez de contar `activeHandles` (número global e ruidoso, sujeito a flakiness), o teste afirma `source.destroyed === true` após a falha no meio da cadeia — que é exatamente a propriedade que `pipeline()` garante e que o `.pipe()` encadeado não garantia.

---

## 1. Sumário executivo

> **Nota de leitura:** desta seção em diante o documento descreve o **diagnóstico original**, no tempo verbal de antes da execução. Ele foi mantido como está para preservar o raciocínio que levou a cada decisão. O que foi efetivamente feito — e o que o diagnóstico errou — está em *Status de implementação* e *Balanço da execução*, acima.

O sistema rodava com `--max-old-space-size=200` num container limitado a 512 MB ([docker-entrypoint.sh](backend/docker-entrypoint.sh), [docker-compose.yml](docker-compose.yml)). Isso **mascarava** o problema em vez de resolvê-lo: o RSS ocioso estava controlado, mas as operações pesadas alocavam sem teto e o processo dependia de sorte para não morrer com `JavaScript heap out of memory` ou `OOMKilled (137)`.

**Diagnóstico central:** o consumo ocioso está bom. O que ameaça a estabilidade são **quatro caminhos sem backpressure e sem limite superior**:

| Caminho                                   | Comportamento hoje                                                   | Consequência                          |
| ----------------------------------------- | -------------------------------------------------------------------- | ------------------------------------- |
| Archive de bucket                         | Abre **todos** os streams de download simultaneamente                | Pico proporcional ao nº de arquivos   |
| Dump de backup                            | `stdout → gzip.write()` ignorando backpressure                        | Fila do gzip cresce sem teto          |
| Filtros de restore                        | Até 5 `Transform` encadeados com buffer de string                    | Até 5 cópias do dump em voo           |
| Retention / audit                         | Carrega tabela inteira em modelos Lucid; audit nunca é podada        | Cresce com a idade da instalação      |

**Meta do roadmap:**

| Métrica                          | Hoje                          | Alvo                              |
| -------------------------------- | ----------------------------- | --------------------------------- |
| RSS ocioso (48 h)                | ~150–200 MB (estimado pós-F1) | **≤ 120 MB**                      |
| Pico durante backup de 10 GB     | Indeterminado (sem teto)      | **≤ 180 MB, constante e previsível** |
| Pico durante archive de 5 k obj. | Indeterminado (risco de OOM)  | **≤ 180 MB, constante**            |
| `--max-old-space-size`           | 200 MB (apertado, arriscado)  | **320 MB com folga real**          |
| Reinícios por OOM                | Possíveis                     | **Zero**                          |

> **Princípio inegociável deste roadmap:** nenhuma otimização pode reduzir a durabilidade ou a integridade de um backup. Onde houver conflito entre "gastar menos RAM" e "garantir que o dump está íntegro", vence a integridade. Cada fase abaixo tem um critério de aceitação funcional além do critério de memória.

---

## 2. Instrumentação primeiro (pré-requisito, não opcional)

Nada abaixo deve ser implementado antes disso. Sem baseline, "otimização" vira adivinhação.

### F0.1 — Corrigir a métrica de memória em container

[system_monitoring_service.ts:147-159](backend/app/services/system_monitoring_service.ts#L147-L159) usa `os.totalmem()` / `os.freemem()`, que reportam a memória do **host**, não o limite do cgroup. Dentro do Docker com `limits.memory: 512M`, o painel mostra números que não têm relação com o que o OOM killer enxerga.

- Ler `/sys/fs/cgroup/memory.max` e `/sys/fs/cgroup/memory.current` (cgroup v2) com fallback para `os.*` fora de container.
- Expor `containerLimitBytes` e `containerUsedBytes` no `/api/system/heap`.
- **Critério:** o painel de RAM do dashboard bate com `docker stats` com erro < 5 %.

### F0.2 — Marcar picos, não só médias

`/api/system/heap` hoje é amostrado a cada 10 s pelo frontend. Um pico de 300 MB durante 4 s é invisível.

- Registrar `rssHighWaterMark` e `heapUsedHighWaterMark` no processo, resetáveis via endpoint.
- Emitir uma linha de log estruturada (`level: warn`) sempre que `heapUsed > 70 %` do limite.
- **Critério:** um backup grande gera evidência no log sem precisar de profiler acoplado.

### F0.3 — Capturar evidência automática antes de morrer

- Adicionar `--heapsnapshot-near-heap-limit=1` ao comando de produção ([docker-entrypoint.sh:51](backend/docker-entrypoint.sh#L51)), gravando em um volume.
- **Critério:** se o processo estourar o heap, existe um `.heapsnapshot` para análise em vez de só um exit 137.

### F0.4 — Cenários de carga reproduzíveis

Criar scripts de carga (fora do runtime de produção) para os quatro caminhos críticos:
1. Backup de um dump de ~5 GB.
2. Restore do mesmo dump com todos os filtros ligados (`schema-only` + `noOwner` + `noPrivileges`).
3. Archive de um storage com ≥ 2 000 objetos.
4. Cópia rclone de longa duração (> 30 min).

- **Critério:** cada cenário produz uma curva de RSS registrada. Essa curva é o **antes** de todas as fases seguintes.

---

## 3. Achados priorizados

Ordenado por *risco de indisponibilidade × esforço*.

| #  | Achado                                                                | Impacto | Esforço | Risco de regressão | Fase |
| -- | --------------------------------------------------------------------- | ------- | ------- | ------------------ | ---- |
| 1  | Archive abre todos os streams de download de uma vez                   | **Crítico** | Médio  | Médio  | F1 |
| 2  | Dump grava no gzip sem respeitar backpressure                          | **Crítico** | Baixo  | Baixo  | F1 |
| 3  | `stderrData` sem limite no `BackupService`                             | Alto    | Trivial | Nenhum | F1 |
| 4  | `stderrBuffer` sem limite no `BucketCopyService` (rclone)              | Alto    | Trivial | Nenhum | F1 |
| 5  | `listAllFiles` materializa o bucket inteiro em memória                 | Alto    | Médio   | Baixo  | F2 |
| 6  | Cadeia de até 5 `Transform` com buffer de string no restore            | Alto    | Médio   | **Alto** | F3 |
| 7  | `.pipe()` sem `pipeline()` — sem destruição em cascata no erro         | Alto    | Baixo   | Baixo  | F2 |
| 8  | `RetentionService` carrega todos os backups como modelos Lucid         | Médio   | Baixo   | Baixo  | F2 |
| 9  | `AuditLog` nunca é podado                                              | Médio   | Baixo   | Baixo  | F2 |
| 10 | `getDecryptedConfig()` faz AES-GCM + `JSON.parse` a cada chamada       | Médio   | Baixo   | Médio  | F2 |
| 11 | Pool SQLite default (até 10 conexões × 4 MB de page cache)             | Médio   | Trivial | Baixo  | F1 |
| 12 | `multipart.limit: 500mb` global com `autoProcess`                      | Médio   | Médio   | Médio  | F4 |
| 13 | `--max-old-space-size=200` apertado demais para as operações pesadas   | Médio   | Trivial | Baixo  | F4 |
| 14 | Broadcast SSE ocorre mesmo sem assinantes                              | Baixo   | Baixo   | Baixo  | F4 |
| 15 | `emitProgress` por arquivo no archive (flood de SSE)                   | Baixo   | Trivial | Nenhum | F1 |
| 16 | Frontend: 17 k snapshots + `JSON.stringify` completo a cada 10 s       | Médio   | Baixo   | Baixo  | F5 |
| 17 | Frontend: bundle único sem `manualChunks`                              | Baixo   | Baixo   | Baixo  | F5 |
| 18 | `docker inspect` de todos os containers a cada ciclo                   | Baixo   | Baixo   | Baixo  | F4 |
| 19 | Typo `desination` na config do logger                                  | Baixo   | Trivial | Nenhum | F4 |

---

## 4. Fases

### F1 — Conter os picos (a fase que elimina o risco de OOM)

Objetivo: **nenhum caminho de código pode alocar proporcionalmente ao tamanho do dado processado.** Toda operação sobre dados grandes precisa ter memória constante.

---

#### F1.1 — Serializar o download no archive (achado 1) — **prioridade máxima**

[bucket_archive_service.ts:137-156](backend/app/services/storage/bucket_archive_service.ts#L137-L156)

```ts
for (const file of allFiles) {
  const downloadResult = await StorageDestinationService.getDownloadStream(storage, file.key)
  archive.append(downloadResult.stream as Readable, { name: file.key })   // ← não espera consumir
  job.processedFiles++
  this.emitProgress(job)
}
```

`archive.append()` **enfileira** a entrada e retorna imediatamente. O `await` da linha anterior só espera o *handshake* do download — não o corpo. Resultado: para um bucket de N arquivos, N respostas HTTP ficam abertas ao mesmo tempo, cada uma com socket, buffers de TLS e o `highWaterMark` do stream. Com 2 000 objetos isso significa 2 000 conexões simultâneas ao S3 e dezenas de MB só de buffers — além de esgotar o pool de sockets do SDK e o limite de file descriptors.

**Correção:** consumir uma entrada por vez, aguardando o evento `entry` do archiver antes de abrir o próximo download. Alternativa equivalente: fila com concorrência limitada (2–4) para não perder throughput de rede.

- **Critério de memória:** RSS durante o archive de 2 000 objetos é **plano** e independe do nº de arquivos.
- **Critério funcional:** o `.tar.gz` gerado contém exatamente os mesmos arquivos e checksums de antes. Testar com bucket de 3 arquivos e com bucket vazio (caminho de [linha 93](backend/app/services/storage/bucket_archive_service.ts#L93)).
- **Guardrail:** medir também o tempo total. Se cair abaixo do aceitável com concorrência 1, subir para 2–4 — não voltar ao comportamento ilimitado.

---

#### F1.2 — Respeitar backpressure no pipeline de dump (achado 2)

[backup_service.ts:683-698](backend/app/services/backup_service.ts#L683-L698)

```ts
stdout.on('data', (data: Buffer) => {
  hash.update(data)
  gzip.write(data)        // ← retorno ignorado
})
```

`gzip.write()` retornando `false` significa "a fila interna passou do limite, pare de escrever". Como o retorno é descartado e o `stdout` nunca é pausado, se o `pg_dump` produzir mais rápido do que o gzip comprime + o disco grava (cenário normal em disco de rede ou volume Docker), **a fila interna do gzip cresce indefinidamente**. Num dump de 10 GB com disco lento, isso é a diferença entre 40 MB e centenas de MB de heap.

O trecho `gzip.pipe(outputStream)` está correto — é só a primeira etapa (`stdout → gzip`) que é manual e sem controle.

**Correção:** usar `stream.pipeline()` com um `Transform` que calcula o hash de passagem, ou tratar `write() === false` pausando o `stdout` e retomando no `drain`.

- **Critério de memória:** backup de 5 GB com disco artificialmente lento (`--limit-rate` ou volume com throttle) mantém RSS plano.
- **Critério funcional:** checksum SHA-256 e tamanho do arquivo idênticos ao comportamento atual — este é o teste que **não pode** falhar. Comparar byte a byte o `.sql.gz` gerado antes e depois.
- **Guardrail:** o `outputStream.on('finish')` de [backup_service.ts:734](backend/app/services/backup_service.ts#L734) resolve a Promise; ao trocar por `pipeline`, garantir que a resolução continua acontecendo **depois** do flush completo do gzip, nunca antes.

---

#### F1.3 — Limitar buffers de stderr ainda não cobertos (achados 3 e 4)

O projeto já tem [`ProcessOutputBuffer`](backend/app/services/process_output_buffer.ts) com truncamento — usado corretamente no `RestoreService` ([restore_service.ts:757-770](backend/app/services/restore_service.ts#L757-L770)) e no monitor Docker. Dois lugares ficaram de fora:

- [backup_service.ts:641](backend/app/services/backup_service.ts#L641) — `let stderrData = ''` com `stderrData += data` a cada chunk. Um `pg_dump` com muitos warnings gera MBs de string.
- [bucket_copy_service.ts:275](backend/app/services/storage/bucket_copy_service.ts#L275) — `let stderrBuffer = ''` acumulando **toda** a saída do rclone. Numa cópia de horas com `--progress`, isso cresce continuamente durante todo o job.

**Correção:** trocar ambos por `ProcessOutputBuffer`.

- **Critério funcional:** a mensagem de erro exibida ao usuário continua útil (o truncamento é no fim, mantendo o início da saída, onde normalmente está a causa raiz).
- **Bônus (achado 4):** [bucket_copy_service.ts:305-307](backend/app/services/storage/bucket_copy_service.ts#L305-L307) faz `logger.debug` por linha de saída do rclone. Em produção `LOG_LEVEL=info` já descarta, mas a string de cada linha é construída antes de ser descartada — mover para trás de uma checagem de nível.

---

#### F1.4 — Fixar o pool do SQLite (achado 11)

[config/database.ts](backend/config/database.ts) define apenas `afterCreate`, herdando o pool default do knex (`min: 2, max: 10`). Combinado com [sqlite_runtime_config.ts](backend/app/services/sqlite_runtime_config.ts), que aplica `cache_size = -4096` (**4 MB por conexão**), o teto é ~40 MB só de page cache — para um banco que é acessado por uma biblioteca **síncrona** (`better-sqlite3`), onde múltiplas conexões não trazem paralelismo nenhum.

**Correção:** `pool: { min: 1, max: 1 }` e reavaliar `cache_size` para `-2048` (2 MB), medindo o efeito nas queries de histórico de métricas.

- **Critério de memória:** −20 a −35 MB de RSS estável.
- **Critério funcional:** [tests/unit/sqlite_runtime_config.spec.ts](backend/tests/unit/sqlite_runtime_config.spec.ts) atualizado; suíte funcional inteira verde; a query de histórico de 15 dias ([resource_metrics_history_service.ts:154-171](backend/app/services/resource_metrics_history_service.ts#L154-L171)) não pode regredir em latência.
- **Atenção:** com `max: 1`, qualquer código que tente duas queries realmente concorrentes serializa. Como `better-sqlite3` já é síncrono, isso não muda o comportamento — mas vale rodar a suíte funcional completa para confirmar que nenhum teste depende de concorrência de pool.

---

#### F1.5 — Throttle no progresso do archive (achado 15)

[bucket_archive_service.ts:151](backend/app/services/storage/bucket_archive_service.ts#L151) emite um broadcast SSE **por arquivo**. Num archive de 5 000 objetos são 5 000 mensagens, cada uma serializando o job inteiro.

**Correção:** aplicar o mesmo padrão de throttle já usado em [backup_progress_emitter.ts:30](backend/app/services/backup_progress_emitter.ts#L30) (`THROTTLE_MS = 500`), sempre emitindo o evento final.

---

### F2 — Eliminar crescimento proporcional ao histórico

Objetivo: **o consumo de memória não pode crescer com a idade da instalação nem com o tamanho do storage.**

#### F2.1 — Paginar a listagem do archive (achado 5)

[bucket_archive_service.ts:182-209](backend/app/services/storage/bucket_archive_service.ts#L182-L209) acumula `allFiles: BucketObject[]` com o bucket inteiro antes de começar a comprimir. Um bucket com 100 k objetos são ~30 MB de metadados retidos durante todo o job.

**Correção:** transformar em iteração *streaming* (async generator), processando cada página conforme chega. `job.totalFiles` passa a ser uma estimativa progressiva — ajustar o contrato do progresso na UI.

- **Dependência:** deve vir **depois** de F1.1, senão a serialização do download some junto com a refatoração.
- **Critério funcional:** a UI de progresso continua coerente (aceitável que o total seja "descoberto" durante a execução — documentar no evento SSE).

#### F2.2 — `pipeline()` em vez de `.pipe()` encadeado (achado 7)

[restore_service.ts:173-188](backend/app/services/restore_service.ts#L173-L188) e [restore_service.ts:404-435](backend/app/services/restore_service.ts#L404-L435) encadeiam `.pipe()` até 5 vezes. `.pipe()` **não propaga destruição**: se o `gunzip` falhar no meio, o `createReadStream` de origem (ou o stream do S3) continua aberto, segurando fd, socket e buffers até o GC — que pode demorar, porque o socket mantém referências vivas.

**Correção:** `stream.pipeline()` (ou `pipeline` de `node:stream/promises`) em toda a cadeia, incluindo [restore_service.ts:810](backend/app/services/restore_service.ts#L810) (`inputStream.pipe(restoreProcess.stdin!)`).

- **Critério funcional:** restore com arquivo corrompido no meio → erro reportado ao usuário **e** nenhum fd vazado (`activeHandles` em `/api/system/heap` volta ao baseline após a falha).
- **Este é o teste que importa:** hoje, uma falha de restore provavelmente vaza handles. Medir `activeHandles` antes e depois de 20 restores que falham.

#### F2.3 — Retention sem carregar a tabela inteira (achado 8)

[retention_service.ts:85-89](backend/app/services/retention_service.ts#L85-L89):

```ts
return await Backup.query()
  .where('protected', false)
  .whereNotIn('status', ['pending', 'running'])
  .orderBy('createdAt', 'desc')          // ← sem limit
```

Cada linha vira um modelo Lucid completo (com `$attributes`, `$original`, `$dirty` e metadados). Numa instalação com 50 k backups isso é um pico de dezenas de MB, 1× por execução do cron de retenção.

**Correção:** selecionar apenas as colunas necessárias pelo planner e processar em lotes (`chunk`/cursor). Avaliar se o `BackupRetentionPlanner` precisa mesmo de modelos completos ou aceita POJOs — o teste [backup_retention_planner.spec.ts](backend/tests/unit/backup_retention_planner.spec.ts) já cobre a lógica, o que torna essa refatoração segura.

- **Critério funcional:** o conjunto de backups promovidos/excluídos é **idêntico** ao do algoritmo atual para o mesmo dataset. Rodar em modo *dry-run* comparando os dois resultados antes de trocar.

#### F2.4 — Retenção de audit logs (achado 9)

Não existe nenhuma poda de `AuditLog` no código. A tabela cresce para sempre; junto com ela crescem o arquivo SQLite, o custo de `VACUUM` e a memória de qualquer query sobre ela.

**Correção:** job de retenção configurável (`AUDIT_RETENTION_DAYS`, default 90), no mesmo cron que já poda `resource_metric_history` ([resource_metrics_history_service.ts:259-274](backend/app/services/resource_metrics_history_service.ts#L259-L274)).

- **Janela definida pelo produto:** o log de auditoria aqui é ferramenta de **diagnóstico recente**, não registro histórico — faixa útil de 15 a 30 dias. O default é 30 (`AUDIT_RETENTION_DAYS`, `0` desliga). A poda nunca é silenciosa: registra em log quantos registros saíram e o corte aplicado.

  > A versão inicial deste roadmap presumia requisito de retenção legal e propunha 90 dias. O produto não tem esse requisito — a janela foi encurtada, o que também reduz o crescimento do SQLite em ~3×.

#### F2.5 — Memoizar configuração descriptografada (achado 10)

[storage_destination.ts:123-127](backend/app/models/storage_destination.ts#L123-L127):

```ts
getDecryptedConfig(): StorageDestinationConfig | null {
  if (!this.configEncrypted) return null
  const json = EncryptionService.decrypt(this.configEncrypted)   // AES-256-GCM
  return JSON.parse(json)
}
```

Cada chamada faz: `Buffer.from(keyHex, 'hex')` ([encryption_service.ts:19-30](backend/app/services/encryption_service.ts#L19-L30)), `createDecipheriv`, decrypt e `JSON.parse` — alocando Buffers e um objeto novo toda vez.

O problema não é o custo unitário, é a frequência: em [bucket_explorer_service.ts:90-135](backend/app/services/storage/bucket_explorer_service.ts#L90-L135), `toRelativeBackupPath()` é chamado **duas vezes por objeto** ([linha 98](backend/app/services/storage/bucket_explorer_service.ts#L98) e [linha 134](backend/app/services/storage/bucket_explorer_service.ts#L134)), e cada chamada descriptografa de novo. Listar 1 000 objetos = 2 000 operações AES + 2 000 `JSON.parse`.

**Correção:** cache por instância do modelo, invalidado quando `configEncrypted` muda. Cachear também a chave derivada em `EncryptionService`.

- **Cuidado de segurança:** o config em claro passa a viver mais tempo no heap. Manter o cache **por instância de modelo** (some com o GC do modelo, escopo de request), nunca num `Map` estático global de longa vida. Não logar nem serializar o valor memoizado.
- **Ganho colateral:** reduz muito o CPU de listagem de storage, que hoje é dominado por cripto desnecessária.

---

### F3 — Refatoração dos filtros de restore (fase de maior risco)

**Isolada de propósito.** Mexer aqui é mexer no caminho que reescreve dados de produção do cliente. Só executar depois de F1 e F2 estáveis, e com cobertura de testes ampliada **antes** da mudança.

#### F3.1 — Colapsar a cadeia de Transforms (achado 6)

[restore_service.ts:404-435](backend/app/services/restore_service.ts#L404-L435) pode encadear até 5 `Transform`, e cada um ([451](backend/app/services/restore_service.ts#L451), [504](backend/app/services/restore_service.ts#L504), [549](backend/app/services/restore_service.ts#L549), [619](backend/app/services/restore_service.ts#L619), [669](backend/app/services/restore_service.ts#L669)) faz:

```ts
buffer += chunk.toString()
const lines = buffer.split('\n')
buffer = lines.pop() || ''
// ...
this.push(output.join('\n') + '\n')
```

Três problemas empilhados:

1. **Memória multiplicada.** Cada estágio mantém seu próprio buffer + array de linhas + string de saída. Com 5 estágios, há até 5 cópias parciais do mesmo trecho de dump em voo simultaneamente.
2. **Buffer sem teto.** Se o dump tiver uma linha muito longa (`INSERT` gigante com blob, ou um bloco `COPY` sem `\n` por muitos MB), `buffer` cresce até essa linha terminar. Não há limite superior.
3. **Bug latente de correção — mais grave que a memória.** `chunk.toString()` sem `StringDecoder` **quebra caracteres UTF-8 multibyte** que caiam na fronteira do chunk. Um acento partido entre dois chunks vira `` no dump restaurado. Isso não é teórico: chunks de 64 KB numa fronteira arbitrária de um dump com texto em português corrompem dados **silenciosamente**.

**Correção:**
- Um único `Transform` que aplica todos os predicados ativos numa passagem.
- `StringDecoder` (`node:string_decoder`) para a fronteira multibyte — **corrigir isso independentemente de qualquer ganho de memória**.
- Limite máximo de linha (ex.: 8 MB) com erro explícito em vez de crescimento indefinido.

- **Critério funcional (bloqueante):** para cada combinação de opções, o SQL de saída deve ser **byte-idêntico** ao do código atual. Construir fixtures com dumps reais de PostgreSQL e MySQL contendo: acentuação, blocos `COPY`, `INSERT` multi-linha, e um chunk cortando um caractere multibyte no meio.
- **Sugestão de ordem:** entregar o `StringDecoder` primeiro, isolado, como correção de bug. Só depois colapsar a cadeia.

---

### F4 — Ajuste fino de runtime e container

#### F4.1 — Recalibrar o heap do V8 (achado 13)

Só executar **depois** de F1–F3, quando os picos forem previsíveis. Hoje, `--max-old-space-size=200` num container de 512 MB é uma aposta: protege contra o OOM killer, mas transforma qualquer pico legítimo em `JavaScript heap out of memory`.

Alvo pós-F1/F2/F3:

| Parâmetro                | Hoje  | Alvo   | Razão                                                                     |
| ------------------------ | ----- | ------ | ------------------------------------------------------------------------- |
| `limits.memory`          | 512M  | 512M   | Mantém                                                                    |
| `--max-old-space-size`   | 200   | 320    | ~65 % do limite; deixa espaço para young gen, code space e buffers nativos |
| `reservations.memory`    | 256M  | 128M   | Alinhado ao RSS ocioso real medido em F0                                  |

> Não aumentar `--max-old-space-size` **antes** de F1. Com os picos atuais sem teto, um heap maior só adia o OOM e piora o diagnóstico.

Sobre `--max-semi-space-size`: mexer só se o profiling de F0 mostrar taxa alta de promoção para old space. Aumentar reduz scavenges mas eleva o RSS base; num container apertado o default costuma ser a escolha certa. **Medir antes, não presumir.**

#### F4.2 — Upload multipart escopado (achado 12)

[config/bodyparser.ts](backend/config/bodyparser.ts) define `limit: '500mb'` com `autoProcess: true` **globalmente** — todas as rotas `POST/PUT/PATCH/DELETE` aceitam multipart de até 500 MB, gravado no tmp do SO.

Dois riscos: se `/tmp` do container for `tmpfs`, o upload vai **direto para a RAM**; e o limite global amplia a superfície de abuso para rotas que nunca deveriam receber arquivos.

**Correção:** manter o limite alto apenas na rota de importação ([backups_controller.ts:353](backend/app/controllers/backups_controller.ts#L353)), reduzir o global para algo como 2 MB, e avaliar `processManually` na rota de import para streamar direto ao destino. Verificar/garantir que `/tmp` no container é disco, não tmpfs.

#### F4.3 — Não fazer broadcast sem assinantes (achado 14)

O ciclo de polling ([resource_metrics_polling_service.ts:79](backend/app/services/resource_metrics_polling_service.ts#L79) e [:93](backend/app/services/resource_metrics_polling_service.ts#L93)) sempre monta o payload e chama `transmit.broadcast`, mesmo com zero clientes conectados — o cenário mais comum (ninguém com o dashboard aberto). A persistência em SQLite deve continuar; só a **construção do payload e o broadcast** podem ser evitados.

Extensão natural: quando não há assinantes por N ciclos, **espaçar o polling** para 30 s e voltar a 10 s na primeira assinatura. Métricas históricas continuam sendo gravadas no intervalo mínimo de 60 s ([resource_metrics_history_service.ts:43](backend/app/services/resource_metrics_history_service.ts#L43)), então não há perda de histórico.

#### F4.4 — Cachear metadados imutáveis do Docker (achado 18)

[docker_container_monitoring_service.ts:505](backend/app/services/docker_container_monitoring_service.ts#L505) roda `docker inspect` com todos os IDs a cada ciclo e faz `JSON.parse` do array inteiro — para extrair apenas nome e labels, que **não mudam** durante a vida do container.

**Correção:** cache por `containerId` com invalidação quando o ID some da listagem.

#### F4.5 — Corrigir typo na config do logger (achado 19)

[config/logger.ts:19](backend/config/logger.ts#L19): `desination:` → `destination:`. Hoje a chave é ignorada; funciona por acaso porque `transport` é `undefined` em produção e o pino cai no stdout. Corrigir e confirmar que **nenhum worker thread de transporte** é criado em produção (worker de pino tem heap próprio contado no RSS).

---

### F5 — Frontend

O navegador do usuário também é "o sistema". Estas mudanças não afetam o RSS do backend, mas afetam a experiência em sessões longas — exatamente o público de uma ferramenta de dashboard que fica aberta o dia todo.

#### F5.1 — Histórico de heap no navegador (achado 16)

[useSystemHeapSnapshots.ts](frontend/src/composables/useSystemHeapSnapshots.ts) com `MAX_STORED_SNAPSHOTS = 20_000` ([linha 25](frontend/src/composables/useSystemHeapSnapshots.ts#L25)) e 48 h de retenção a cada 10 s = ~17 280 snapshots. A cada poll ([linhas 57-64](frontend/src/composables/useSystemHeapSnapshots.ts#L57-L64)) o código faz:

1. `history.value.filter(...)` — cópia do array (17 k objetos)
2. `pruneSnapshots` → `[...snapshots].filter().sort().slice()` — mais **três** cópias, incluindo um `sort` completo de 17 k itens
3. `JSON.stringify(nextHistory)` → `localStorage.setItem` — serializa ~3 MB de string **a cada 10 segundos**

Isso é lixo de heap contínuo na thread principal, além de estourar a cota típica de 5 MB do `localStorage` (o erro é engolido pelo `catch {}` da [linha 142](frontend/src/composables/useSystemHeapSnapshots.ts#L142), então falha em silêncio).

**Correção:** reduzir para ~1 500 pontos com downsampling progressivo (resolução fina nas últimas horas, agregada no resto); persistir no máximo 1× por minuto; inserção incremental em vez de recriar o array; considerar mover a série para o backend, que já tem a tabela de histórico.

#### F5.2 — Code splitting (achado 17)

[vite.config.mts:132-135](frontend/vite.config.mts#L132-L135) não define `manualChunks`. Vuetify inteiro + todas as páginas (Docker manager, storages, audit, charts) entram no bundle inicial. Separar por rota reduz a memória de parse/compile do JS no dispositivo do usuário.

#### F5.3 — Disciplina de listeners

[stores/notification.ts:34](frontend/src/stores/notification.ts#L34) mantém um `Map` de listeners numa store Pinia (singleton). Hoje só [pages/backups/index.vue:616-617](frontend/src/pages/backups/index.vue#L616-L617) chama `offNotification` no unmount. O padrão está correto, mas depende de disciplina manual: qualquer `onNotification` novo sem o `off` correspondente vaza o componente inteiro (closure → componente → subtree do DOM).

**Correção:** encapsular num composable `useNotificationListener()` que registra e desregistra automaticamente via `onScopeDispose`, tornando o vazamento impossível por construção.

---

## 5. Boas práticas a institucionalizar

Para que os ganhos não regridam, estas regras deveriam virar critério de revisão de PR:

### Streams
- Nunca `.pipe()` encadeado — sempre `pipeline()`, que destrói toda a cadeia em caso de erro.
- Nunca `write()` com retorno ignorado num caminho de dados grandes.
- Nunca acumular saída de processo em `string +=` — usar `ProcessOutputBuffer`.
- `chunk.toString()` em stream de texto exige `StringDecoder`.
- Toda operação sobre coleção externa (bucket, tabela, diretório) tem concorrência **explícita e limitada**.

### Memória de estado
- Todo `Map`/array de vida longa precisa de teto **e** de TTL. O padrão já usado em [s3_client_registry.ts](backend/app/services/storage/s3_client_registry.ts) (TTL + LRU + `destroy()`) é o modelo a seguir.
- Toda tabela que só cresce precisa de job de retenção desde o primeiro dia.
- Query que pode retornar N linhas ilimitadas precisa de `limit` ou de processamento em lotes.

### Timers e SSE
- `setInterval` de longa vida sempre com `.unref()` — já aplicado em [bucket_archive_service.ts:247](backend/app/services/storage/bucket_archive_service.ts#L247) e [bucket_copy_service.ts:366](backend/app/services/storage/bucket_copy_service.ts#L366); manter como padrão.
- Todo emitter de progresso com throttle; evento final sempre emitido.
- Não construir payload de broadcast sem assinantes.

### Runtime
- `--max-old-space-size` sempre definido explicitamente e sempre coerente com `limits.memory` (~65 %).
- Ler limites de cgroup, nunca `os.totalmem()`, quando containerizado.
- Pool de conexão do SQLite fixo em 1 (a lib é síncrona).

---

## 6. Guardrails de regressão

O projeto já tem [tests/unit/performance_quick_wins.spec.ts](backend/tests/unit/performance_quick_wins.spec.ts) protegendo os ganhos da primeira rodada. Estender com:

| Teste                                                     | Protege                     | Situação |
| --------------------------------------------------------- | --------------------------- | -------- |
| Archive de N objetos abre no máximo K downloads simultâneos | F1.1                        | ✅ |
| Dump pausa o stdout quando o consumidor satura             | F1.2                        | ⬜ não determinístico em unit test — ver *Pendências* |
| Checksum do `.sql.gz` idêntico ao baseline                  | F1.2 (integridade)          | ✅ |
| stderr do dump truncado no limite                           | F1.3                        | ✅ |
| stderr do rclone truncado no limite                         | F1.3                        | ⬜ exigiria `rclone` falso no PATH |
| Falha de restore não deixa handles ativos                  | F2.2                        | ✅ (via `source.destroyed`) |
| Filtros produzem saída byte-idêntica, incluindo UTF-8 partido | F3.1 (integridade)        | ✅ |
| Pool SQLite = 1 conexão                                     | F1.4                        | ✅ |
| Poda de audit respeita `AUDIT_RETENTION_DAYS`               | F2.4                        | ✅ |
| Upload multipart continua sendo auto-processado             | F4.2                        | ✅ |

Além disso: um cenário de carga no CI (ou executado manualmente antes de cada release) que roda os quatro fluxos de F0.4 e falha se o RSS de pico exceder o orçamento definido.

---

## 7. Sequenciamento e risco

```
F0  Instrumentação        ──►  obrigatório antes de tudo
     │
F1  Contenção de picos    ──►  elimina o risco de OOM   [maior ganho, menor risco]
     │
F2  Crescimento histórico ──►  elimina degradação com o tempo
     │
F3  Filtros de restore    ──►  isolado; corrige bug de UTF-8   [maior risco]
     │
F4  Runtime / container   ──►  só faz sentido com picos já previsíveis
     │
F5  Frontend              ──►  independente; pode correr em paralelo a F1–F4
```

**Matriz de risco das mudanças:**

| Fase | Risco para os dados | Mitigação                                                     |
| ---- | ------------------- | ------------------------------------------------------------- |
| F1.1 | Baixo               | Comparar manifesto do tar antes/depois                        |
| F1.2 | **Médio**           | Comparação byte a byte do `.sql.gz`; checksum é o gate         |
| F1.4 | Baixo               | Suíte funcional completa                                      |
| F2.3 | Médio               | Dry-run comparando o plano antigo e o novo                    |
| F2.4 | Baixo | Janela de diagnóstico (30 dias), configurável, poda registrada em log |
| F2.5 | Baixo (segurança)   | Cache por instância, nunca global; sem log do valor            |
| F3.1 | **Alto**            | Fixtures byte-idênticas; entregar `StringDecoder` isolado antes |
| F4.1 | Baixo               | Reversível por variável de ambiente                           |

**F5 não bloqueia nada** — pode ser feito em paralelo por quem estiver no frontend.

---

## 8. Resultado esperado

Com F0–F4 concluídas:

- RSS ocioso estável em **≤ 120 MB** (de ~150–200 MB).
- Picos de backup, restore e archive **planos e independentes do tamanho do dado** — o ganho que realmente importa, porque hoje esses picos não têm teto conhecido.
- Heap V8 em 320 MB com folga real dentro do limite de 512 MB, em vez de 200 MB no limite da corda.
- Zero crescimento com a idade da instalação (audit e retention podados).
- Um bug silencioso de corrupção UTF-8 no restore corrigido (F3.1) — provavelmente o achado mais valioso desta avaliação, mesmo não sendo sobre memória.

---

## Anexo — Comandos de diagnóstico

```bash
# RSS real do container (compara com o painel após F0.1)
docker stats --no-stream backup-multi-db-backend

# Limite e uso via cgroup v2, de dentro do container
cat /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.current

# Heap snapshot ao se aproximar do limite (F0.3)
node --max-old-space-size=320 --heapsnapshot-near-heap-limit=1 bin/server.js

# Picos de RSS/heap acumulados — saem no warn de pressão acima de 70% (F0.2)
docker logs backup-multi-db-backend 2>&1 | grep '\[Memory\]'

# Verificar se /tmp é tmpfs (RAM) — relevante para F4.2
df -h /tmp && mount | grep ' /tmp '
```
