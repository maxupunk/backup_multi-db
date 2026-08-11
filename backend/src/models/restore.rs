//! Restauracao de um backup para um banco de destino.
//!
//! Porte de `app/services/restore_service.ts`. O caminho e' o inverso do dump:
//! arquivo → gunzip → filtros de linha → `stdin` do `psql`/`mysql`.
//!
//! ## Os filtros trabalham em **bytes**, nao em texto
//!
//! O Adonis converte cada pedaco em `string` com um `StringDecoder` justamente
//! para nao partir um caractere UTF-8 na fronteira entre chunks — o bug que os
//! testes `restore_filters.spec.ts` fixaram, e que corrompia acentuacao no banco
//! restaurado sem nenhum erro visivel.
//!
//! Aqui o problema simplesmente nao existe: as regras decidem sobre `&[u8]`, e
//! o unico separador procurado e' `\n` (`0x0A`), que **nao** aparece dentro de
//! nenhuma sequencia multibyte de UTF-8. Nao ha' conversao, nao ha' fronteira
//! para errar, e a saida e' byte a byte igual a' entrada nas linhas mantidas —
//! inclusive para dumps em Latin-1, que o decodificador do Node teria
//! substituido por `U+FFFD`.
//!
//! ## Por que existe `keep_trailing` separado de `keep_line`
//!
//! O resto sem quebra de linha final e' avaliado por outra regra. Nao e'
//! elegante, e' compatibilidade: na implementacao encadeada do Adonis os
//! filtros `data-only` emitiam esse resto **sem** reavaliar a allowlist, e
//! mudar isso alteraria a saida de um dump que nao termine em `\n`.
//!
//! ## `clear_database` usa o driver, nao o CLI
//!
//! O Adonis da' `spawn` em `psql`/`mysql` so' para rodar tres comandos de DDL.
//! Pelo [`database_driver`](crate::models::database_driver) nao ha' processo
//! filho nem dependencia de binario no PATH — mesma divergencia deliberada ja'
//! registrada na Fase 6 para `create-database`. O restore em si continua pelo
//! CLI: nenhum driver executa um dump com centenas de milhares de instrucoes de
//! forma confiavel.

use std::path::Path;
use std::pin::Pin;
use std::sync::LazyLock;
use std::task::{Context, Poll};

use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::process::Command;

use crate::models::connections::DatabaseType;
use crate::models::database_driver::DatabaseTarget;
use crate::models::process_output::{drain, DEFAULT_LIMIT_BYTES};

/// Teto de uma unica linha do dump mantida em memoria.
///
/// Os filtros trabalham por linha, entao um dump inteiro numa linha so' faria o
/// buffer crescer sem limite ate' estourar o heap. O valor e' folgado de
/// proposito: dumps reais, inclusive `mysqldump --extended-insert` com blobs,
/// ficam muito abaixo — e a mensagem de erro indica a saida, que e' restaurar
/// em modo `full` sem opcoes, onde nenhum filtro processa por linha.
const MAX_BUFFERED_LINE_BYTES: usize = 64 * 1024 * 1024;

/// Modo de restauracao.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreMode {
    #[default]
    Full,
    SchemaOnly,
    DataOnly,
}

impl RestoreMode {
    pub const CHOICES: [&'static str; 3] = ["full", "schema-only", "data-only"];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::SchemaOnly => "schema-only",
            Self::DataOnly => "data-only",
        }
    }
}

impl std::str::FromStr for RestoreMode {
    type Err = ();

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        match input {
            "full" => Ok(Self::Full),
            "schema-only" => Ok(Self::SchemaOnly),
            "data-only" => Ok(Self::DataOnly),
            _ => Err(()),
        }
    }
}

/// Opcoes de compatibilidade de uma restauracao.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOptions {
    #[serde(default)]
    pub mode: RestoreMode,
    pub target_database: Option<String>,
    /// PostgreSQL: descarta `ALTER … OWNER TO`.
    #[serde(default)]
    pub no_owner: bool,
    /// PostgreSQL: descarta `GRANT`/`REVOKE`.
    #[serde(default)]
    pub no_privileges: bool,
    /// PostgreSQL: descarta `SET default_tablespace`.
    #[serde(default)]
    pub no_tablespaces: bool,
    /// PostgreSQL: descarta `COMMENT ON`.
    #[serde(default)]
    pub no_comments: bool,
    /// MySQL/MariaDB: descarta `CREATE DATABASE` e `USE`.
    #[serde(default)]
    pub no_create_db: bool,
    /// Pula a checagem e o backup de seguranca previos.
    #[serde(default)]
    pub skip_safety_backup: bool,
    /// Esvazia o destino antes de restaurar.
    #[serde(default)]
    pub clear_before_restore: bool,
}

// ============================================================================
// Filtros de linha
// ============================================================================

macro_rules! pattern {
    ($name:ident, $source:literal) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| {
            // `expect` em constante compilada uma unica vez no primeiro uso: um
            // padrao invalido aqui e' erro de programacao, nao de entrada.
            Regex::new($source).expect("padrao de filtro de restore invalido")
        });
    };
}

pattern!(COPY_FROM_STDIN, r"(?i)^COPY\s+.*\s+FROM\s+stdin");
pattern!(INSERT_INTO, r"(?i)^\s*INSERT\s+INTO\s+");
pattern!(LOCK_TABLES, r"(?i)^\s*LOCK\s+TABLES\s+");
pattern!(UNLOCK_TABLES, r"(?i)^\s*UNLOCK\s+TABLES");
pattern!(
    MYSQL_LOCK_OR_UNLOCK,
    r"(?i)^\s*(LOCK\s+TABLES|UNLOCK\s+TABLES)"
);
pattern!(
    MYSQL_SCHEMA_ONLY_TRAILING,
    r"(?i)^\s*(INSERT\s+INTO|LOCK\s+TABLES|UNLOCK\s+TABLES)"
);
pattern!(
    PG_SESSION_SETUP,
    r"(?i)^\s*(SET|BEGIN|COMMIT|ROLLBACK|SELECT\s+pg_catalog\.set_config)"
);
pattern!(
    PG_TRIGGER_TOGGLE,
    r"(?i)^\s*ALTER\s+TABLE\s+.*\s+(DISABLE|ENABLE)\s+TRIGGER"
);
pattern!(MYSQL_SET, r"(?i)^\s*SET\s+");
pattern!(MYSQL_VERSIONED_SET, r"(?i)^\s*/\*!\d+\s+SET\s+");
pattern!(SQL_LINE_COMMENT, r"^\s*--");
pattern!(SQL_BLOCK_COMMENT, r"^\s*/\*");
pattern!(PG_OWNER, r"(?i)^\s*ALTER\s+.*\s+OWNER\s+TO\s+");
pattern!(PG_GRANT_OR_REVOKE, r"(?i)^\s*(GRANT|REVOKE)\s+");
pattern!(
    PG_DEFAULT_TABLESPACE,
    r"(?i)^\s*SET\s+default_tablespace\s*="
);
pattern!(PG_COMMENT_ON, r"(?i)^\s*COMMENT\s+ON\s+");
pattern!(
    MYSQL_CREATE_DB_OR_USE,
    r"(?i)^\s*(CREATE\s+DATABASE|USE\s+`)"
);

/// Fim de um bloco `COPY … FROM stdin` do PostgreSQL.
const COPY_TERMINATOR: &[u8] = b"\\.";

/// Regras derivadas do modo de restauracao.
///
/// O sufixo repetido e' proposital: os nomes espelham `schema-only` e
/// `data-only` do contrato, e encurtar para `PostgresSchema` afastaria o codigo
/// do vocabulario que o cliente envia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum ModeRules {
    PostgresSchemaOnly,
    MysqlSchemaOnly,
    PostgresDataOnly,
    MysqlDataOnly,
}

impl ModeRules {
    const fn resolve(kind: DatabaseType, mode: RestoreMode) -> Option<Self> {
        let postgres = matches!(kind, DatabaseType::Postgresql);

        match mode {
            RestoreMode::Full => None,
            RestoreMode::SchemaOnly => Some(if postgres {
                Self::PostgresSchemaOnly
            } else {
                Self::MysqlSchemaOnly
            }),
            RestoreMode::DataOnly => Some(if postgres {
                Self::PostgresDataOnly
            } else {
                Self::MysqlDataOnly
            }),
        }
    }
}

/// Decide, linha a linha, o que chega ao banco de destino.
///
/// Guarda estado: os blocos `COPY … FROM stdin` do PostgreSQL so' terminam na
/// linha `\.`, e sem lembrar que esta dentro de um deles o filtro `schema-only`
/// deixaria os dados passarem.
#[derive(Debug)]
pub struct LineFilter {
    mode: Option<ModeRules>,
    drop_patterns: Vec<&'static LazyLock<Regex>>,
    inside_copy_block: bool,
}

impl LineFilter {
    /// Monta o filtro, ou `None` quando nenhuma opcao filtra nada.
    ///
    /// Devolver `None` nao e' otimizacao cosmetica: sem filtro o dump vai do
    /// arquivo para o `stdin` sem ser quebrado em linhas, e a restauracao
    /// completa deixa de ter qualquer teto de memoria por linha.
    #[must_use]
    pub fn build(kind: DatabaseType, options: &RestoreOptions) -> Option<Self> {
        let mode = ModeRules::resolve(kind, options.mode);
        let drop_patterns = Self::drop_patterns(kind, options);

        if mode.is_none() && drop_patterns.is_empty() {
            return None;
        }

        Some(Self {
            mode,
            drop_patterns,
            inside_copy_block: false,
        })
    }

    fn drop_patterns(
        kind: DatabaseType,
        options: &RestoreOptions,
    ) -> Vec<&'static LazyLock<Regex>> {
        let mut patterns: Vec<&'static LazyLock<Regex>> = Vec::new();

        match kind {
            DatabaseType::Postgresql => {
                if options.no_owner {
                    patterns.push(&PG_OWNER);
                }
                if options.no_privileges {
                    patterns.push(&PG_GRANT_OR_REVOKE);
                }
                if options.no_tablespaces {
                    patterns.push(&PG_DEFAULT_TABLESPACE);
                }
                if options.no_comments {
                    patterns.push(&PG_COMMENT_ON);
                }
            }
            DatabaseType::Mysql | DatabaseType::Mariadb => {
                if options.no_create_db {
                    patterns.push(&MYSQL_CREATE_DB_OR_USE);
                }
            }
        }

        patterns
    }

    fn is_dropped(&self, line: &[u8]) -> bool {
        self.drop_patterns
            .iter()
            .any(|pattern| pattern.is_match(line))
    }

    /// Decide sobre uma linha completa, podendo mudar de estado.
    pub fn keep_line(&mut self, line: &[u8]) -> bool {
        let kept = match self.mode {
            None => true,
            Some(ModeRules::PostgresSchemaOnly) => self.postgres_schema_only(line),
            Some(ModeRules::MysqlSchemaOnly) => Self::mysql_schema_only(line),
            Some(ModeRules::PostgresDataOnly) => self.postgres_data_only(line),
            Some(ModeRules::MysqlDataOnly) => Self::mysql_data_only(line),
        };

        kept && !self.is_dropped(line)
    }

    /// Decide sobre o resto final sem quebra de linha.
    #[must_use]
    pub fn keep_trailing(&self, line: &[u8]) -> bool {
        let kept = match self.mode {
            None => true,
            Some(ModeRules::PostgresSchemaOnly) => {
                !self.inside_copy_block && !INSERT_INTO.is_match(line)
            }
            Some(ModeRules::MysqlSchemaOnly) => !MYSQL_SCHEMA_ONLY_TRAILING.is_match(line),
            // Compatibilidade: os filtros `data-only` do Adonis emitiam o resto
            // sem reavaliar a allowlist.
            Some(ModeRules::PostgresDataOnly | ModeRules::MysqlDataOnly) => true,
        };

        kept && !self.is_dropped(line)
    }

    fn postgres_schema_only(&mut self, line: &[u8]) -> bool {
        if self.inside_copy_block {
            if line == COPY_TERMINATOR {
                self.inside_copy_block = false;
            }
            return false;
        }

        if COPY_FROM_STDIN.is_match(line) {
            self.inside_copy_block = true;
            return false;
        }

        !INSERT_INTO.is_match(line)
    }

    fn mysql_schema_only(line: &[u8]) -> bool {
        !INSERT_INTO.is_match(line) && !LOCK_TABLES.is_match(line) && !UNLOCK_TABLES.is_match(line)
    }

    fn postgres_data_only(&mut self, line: &[u8]) -> bool {
        if self.inside_copy_block {
            if line == COPY_TERMINATOR {
                self.inside_copy_block = false;
            }
            return true;
        }

        if COPY_FROM_STDIN.is_match(line) {
            self.inside_copy_block = true;
            return true;
        }

        INSERT_INTO.is_match(line)
            || PG_SESSION_SETUP.is_match(line)
            || SQL_LINE_COMMENT.is_match(line)
            || line.trim_ascii().is_empty()
            // O `COPY` exige os triggers desligados; descartar estas linhas
            // faria a restauracao de dados falhar em toda tabela com FK.
            || PG_TRIGGER_TOGGLE.is_match(line)
    }

    fn mysql_data_only(line: &[u8]) -> bool {
        INSERT_INTO.is_match(line)
            || MYSQL_LOCK_OR_UNLOCK.is_match(line)
            || MYSQL_SET.is_match(line)
            || MYSQL_VERSIONED_SET.is_match(line)
            || SQL_LINE_COMMENT.is_match(line)
            || SQL_BLOCK_COMMENT.is_match(line)
            || line.trim_ascii().is_empty()
    }
}

/// Adaptador de escrita que aplica um [`LineFilter`] ao que passa por ele.
///
/// E' o equivalente do `Transform` do Node, do lado de escrita: entra no
/// `tokio::io::copy` entre o gunzip e o `stdin` do cliente, mantendo o
/// backpressure. O resto parcial so' sai no `shutdown`, que e' onde a regra de
/// [`LineFilter::keep_trailing`] se aplica.
pub struct FilterWriter<W> {
    inner: W,
    filter: LineFilter,
    /// Linha incompleta herdada do bloco anterior, **antes** do filtro.
    pending: Vec<u8>,
    /// Bytes ja' aprovados que o destino ainda nao aceitou.
    ///
    /// Existe porque um `poll_write` no `stdin` de um processo pode aceitar
    /// menos do que foi oferecido: o pipe tem buffer finito, e o `psql` do outro
    /// lado consome no ritmo dele. Sem esta fila, o resto voltaria para
    /// `pending` e seria filtrado uma segunda vez — linhas ja' aprovadas
    /// passariam de novo pelas regras, e um bloco `COPY` cortado no meio
    /// mudaria o estado do filtro.
    outbox: Vec<u8>,
    /// O resto sem quebra final ja' foi resolvido?
    trailing_flushed: bool,
    max_line_bytes: usize,
}

impl<W> FilterWriter<W> {
    pub const fn new(inner: W, filter: LineFilter) -> Self {
        Self {
            inner,
            filter,
            pending: Vec::new(),
            outbox: Vec::new(),
            trailing_flushed: false,
            max_line_bytes: MAX_BUFFERED_LINE_BYTES,
        }
    }

    #[cfg(test)]
    const fn with_max_line_bytes(mut self, max: usize) -> Self {
        self.max_line_bytes = max;
        self
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: AsyncWrite + Unpin> FilterWriter<W> {
    /// Aplica o filtro a um bloco e devolve o que deve seguir adiante.
    fn select(&mut self, chunk: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut kept = Vec::with_capacity(chunk.len());
        let mut rest = chunk;

        while let Some(position) = rest.iter().position(|byte| *byte == b'\n') {
            let (line, remainder) = rest.split_at(position);
            rest = &remainder[1..];

            // Uma linha inteira dentro de um bloco so' evita a copia para o
            // buffer, que e' o caso comum.
            if self.pending.is_empty() {
                if self.filter.keep_line(line) {
                    kept.extend_from_slice(line);
                    kept.push(b'\n');
                }
            } else {
                self.pending.extend_from_slice(line);
                if self.filter.keep_line(&self.pending) {
                    kept.extend_from_slice(&self.pending);
                    kept.push(b'\n');
                }
                self.pending.clear();
            }
        }

        self.pending.extend_from_slice(rest);

        if self.pending.len() > self.max_line_bytes {
            return Err(std::io::Error::other(format!(
                "Linha do dump excede {} MB sem quebra de linha. \
                 Restaure em modo \"full\" sem opções de compatibilidade para \
                 evitar o processamento por linha.",
                self.max_line_bytes / (1024 * 1024)
            )));
        }

        Ok(kept)
    }

    /// Empurra a fila de saida para o destino ate' esvazia-la.
    fn poll_drain(&mut self, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        while !self.outbox.is_empty() {
            let written =
                std::task::ready!(Pin::new(&mut self.inner).poll_write(cx, &self.outbox))?;

            if written == 0 {
                return Poll::Ready(Err(std::io::Error::from(std::io::ErrorKind::WriteZero)));
            }

            self.outbox.drain(..written);
        }

        Poll::Ready(Ok(()))
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for FilterWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        // A fila precisa estar vazia antes de aceitar mais entrada; do
        // contrario ela cresceria sem teto quando o `psql` consumisse mais
        // devagar do que o gunzip produz.
        std::task::ready!(this.poll_drain(cx))?;

        let kept = this.select(buf)?;
        this.outbox.extend_from_slice(&kept);

        // O que nao couber agora fica na fila para a proxima chamada; o bloco de
        // entrada foi todo consumido de qualquer forma.
        if let Poll::Ready(Err(err)) = this.poll_drain(cx) {
            return Poll::Ready(Err(err));
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        std::task::ready!(this.poll_drain(cx))?;

        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        std::task::ready!(this.poll_drain(cx))?;

        // Idempotente: `poll_shutdown` pode ser chamado varias vezes depois de
        // um `Pending`, e reavaliar o resto passaria a linha final duas vezes.
        if !this.trailing_flushed {
            this.trailing_flushed = true;

            if !this.pending.is_empty() {
                let trailing = std::mem::take(&mut this.pending);

                if this.filter.keep_trailing(&trailing) {
                    this.outbox.extend_from_slice(&trailing);
                    this.outbox.push(b'\n');
                }
            }
        }

        std::task::ready!(this.poll_drain(cx))?;

        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

// ============================================================================
// Execucao
// ============================================================================

/// Comando de restore, montado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreCommand {
    pub program: &'static str,
    pub args: Vec<String>,
    pub env: Vec<(&'static str, String)>,
}

/// Monta a linha de comando do cliente que recebe o dump pelo `stdin`.
#[must_use]
pub fn build_command(target: &DatabaseTarget, database: &str) -> RestoreCommand {
    match target.kind {
        DatabaseType::Postgresql => RestoreCommand {
            program: "psql",
            args: vec![
                "-h".to_string(),
                target.host.clone(),
                "-p".to_string(),
                target.port.to_string(),
                "-U".to_string(),
                target.username.clone(),
                "-d".to_string(),
                database.to_string(),
                "--no-password".to_string(),
                // Sem isto o `psql` segue adiante depois de um erro e termina
                // com codigo 0: a restauracao apareceria como concluida com o
                // banco pela metade.
                "-v".to_string(),
                "ON_ERROR_STOP=1".to_string(),
            ],
            env: vec![("PGPASSWORD", target.password.clone())],
        },
        DatabaseType::Mysql | DatabaseType::Mariadb => {
            let mut args = vec![
                "-h".to_string(),
                target.host.clone(),
                "-P".to_string(),
                target.port.to_string(),
                "-u".to_string(),
                target.username.clone(),
                format!("--password={}", target.password),
            ];

            if !target.ssl {
                args.push("--skip-ssl".to_string());
            }

            args.push(database.to_string());

            RestoreCommand {
                program: "mysql",
                args,
                env: Vec::new(),
            }
        }
    }
}

/// Desfecho de uma restauracao.
#[derive(Debug, Clone)]
pub struct RestoreOutcome {
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error(
        "Falha ao executar {program}: {source}. Verifique se o binário está instalado e no PATH."
    )]
    Spawn {
        program: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("Erro ao ler arquivo de backup: {0}")]
    Source(#[source] std::io::Error),
    #[error("{message}")]
    Failed {
        message: String,
        exit_code: Option<i64>,
    },
}

/// Le' o `.sql.gz` (ou `.sql`) e alimenta o cliente de banco.
///
/// `on_progress` recebe os bytes lidos **do arquivo**, antes da descompressao —
/// e' o unico total conhecido de antemao (`backups.file_size`), e medir depois
/// do gunzip daria um percentual que passa de 100%.
pub async fn execute<F>(
    command: &RestoreCommand,
    source_path: &Path,
    compressed: bool,
    filter: Option<LineFilter>,
    on_progress: F,
) -> std::result::Result<RestoreOutcome, RestoreError>
where
    F: FnMut(u64) + Send + Unpin + 'static,
{
    let file = tokio::fs::File::open(source_path)
        .await
        .map_err(RestoreError::Source)?;

    execute_from_reader(command, file, compressed, filter, on_progress).await
}

/// Igual a [`execute`], mas lendo de qualquer origem.
///
/// Foi o que permitiu restaurar de um **destino remoto** sem passar por um
/// arquivo temporário (tarefa 7.6): o objeto do bucket entra direto na mesma
/// cadeia. Baixar para o disco antes custaria uma gravação inteira do dump,
/// espaço que a máquina pode não ter, e um arquivo a limpar depois.
pub async fn execute_from_reader<R, F>(
    command: &RestoreCommand,
    source: R,
    compressed: bool,
    filter: Option<LineFilter>,
    on_progress: F,
) -> std::result::Result<RestoreOutcome, RestoreError>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    F: FnMut(u64) + Send + Unpin + 'static,
{
    let mut child = Command::new(command.program)
        .args(&command.args)
        .envs(command.env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| RestoreError::Spawn {
            program: command.program,
            source,
        })?;

    let stdin = child.stdin.take().ok_or_else(|| {
        RestoreError::Source(std::io::Error::other("o cliente de banco não expôs stdin"))
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // As duas saidas sao drenadas em paralelo com a escrita. Ler depois faria o
    // `psql` travar assim que o pipe enchesse, e um dump grande com muitos
    // avisos enche.
    let stdout_task = tokio::spawn(async move {
        match stdout {
            Some(pipe) => drain(pipe, DEFAULT_LIMIT_BYTES).await.to_text(),
            None => String::new(),
        }
    });
    let stderr_task = tokio::spawn(async move {
        match stderr {
            Some(pipe) => drain(pipe, DEFAULT_LIMIT_BYTES).await.to_text(),
            None => String::new(),
        }
    });

    let stream_result = feed(source, compressed, filter, stdin, on_progress).await;

    let status = child.wait().await.map_err(RestoreError::Source)?;
    let stdout_text = stdout_task.await.unwrap_or_default();
    let stderr_text = stderr_task.await.unwrap_or_default();
    let warnings = extract_warnings(&stderr_text);
    let exit_code = status.code().map(i64::from);

    // O desfecho do processo tem precedencia sobre o erro de stream. Quando o
    // banco aborta no meio (`psql` com `ON_ERROR_STOP=1`), a escrita restante no
    // stdin falha — EPIPE no Linux, EOF no Windows. Classificar esse erro pelo
    // codigo seria fragil; o stderr do banco explica a causa real.
    if !status.success() {
        let message = [stderr_text.as_str(), stdout_text.as_str()]
            .into_iter()
            .find(|text| !text.trim().is_empty())
            .map_or_else(
                || {
                    format!(
                        "Processo terminou com código {}",
                        exit_code
                            .map_or_else(|| "desconhecido".to_string(), |code| code.to_string())
                    )
                },
                ToString::to_string,
            );

        return Err(RestoreError::Failed { message, exit_code });
    }

    // Processo terminou bem, mas a cadeia quebrou: o dump foi entregue pela
    // metade (arquivo corrompido, download interrompido). Reportar sucesso aqui
    // seria mentir sobre o estado do banco restaurado.
    stream_result?;

    Ok(RestoreOutcome { warnings })
}

/// Encadeia origem → progresso → gunzip → filtro → `stdin`.
async fn feed<R, F>(
    source: R,
    compressed: bool,
    filter: Option<LineFilter>,
    stdin: tokio::process::ChildStdin,
    on_progress: F,
) -> std::result::Result<(), RestoreError>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    F: FnMut(u64) + Send + Unpin + 'static,
{
    let counted = ProgressReader::new(tokio::io::BufReader::new(source), on_progress);

    // O `match` duplica o corpo porque os dois ramos tem tipos diferentes de
    // leitor; embrulhar num `Box<dyn AsyncRead>` custaria uma indirecao por
    // bloco para poupar quatro linhas.
    if compressed {
        let mut decoder = async_compression::tokio::bufread::GzipDecoder::new(counted);
        pump(&mut decoder, filter, stdin).await
    } else {
        let mut plain = counted;
        pump(&mut plain, filter, stdin).await
    }
}

async fn pump<R>(
    source: &mut R,
    filter: Option<LineFilter>,
    stdin: tokio::process::ChildStdin,
) -> std::result::Result<(), RestoreError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    match filter {
        Some(filter) => {
            let mut writer = FilterWriter::new(stdin, filter);
            tokio::io::copy(source, &mut writer)
                .await
                .map_err(RestoreError::Source)?;
            // Fecha o `stdin` do cliente **e** emite o resto sem quebra final.
            // Sem o `shutdown`, o `psql` ficaria esperando mais entrada para
            // sempre.
            writer.shutdown().await.map_err(RestoreError::Source)?;
        }
        None => {
            let mut stdin = stdin;
            tokio::io::copy(source, &mut stdin)
                .await
                .map_err(RestoreError::Source)?;
            stdin.shutdown().await.map_err(RestoreError::Source)?;
        }
    }

    Ok(())
}

/// Linhas do `stderr` que sao aviso, e nao erro.
fn extract_warnings(stderr: &str) -> Vec<String> {
    if stderr.is_empty() {
        return Vec::new();
    }

    stderr
        .lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("warning") || lower.contains("notice") || lower.contains("info")
        })
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Leitor que informa quantos bytes ja' passaram.
struct ProgressReader<R, F> {
    inner: R,
    bytes: u64,
    on_progress: F,
}

impl<R, F> ProgressReader<R, F> {
    const fn new(inner: R, on_progress: F) -> Self {
        Self {
            inner,
            bytes: 0,
            on_progress,
        }
    }
}

impl<R: tokio::io::AsyncRead + Unpin, F: FnMut(u64) + Unpin> tokio::io::AsyncRead
    for ProgressReader<R, F>
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();

        std::task::ready!(Pin::new(&mut this.inner).poll_read(cx, buf))?;

        let read = buf.filled().len() - before;
        if read > 0 {
            this.bytes += read as u64;
            (this.on_progress)(this.bytes);
        }

        Poll::Ready(Ok(()))
    }
}

impl<R: tokio::io::AsyncBufRead + Unpin, F: FnMut(u64) + Unpin> tokio::io::AsyncBufRead
    for ProgressReader<R, F>
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        Pin::new(&mut self.get_mut().inner).poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amount: usize) {
        let this = self.get_mut();
        this.bytes += amount as u64;
        (this.on_progress)(this.bytes);
        Pin::new(&mut this.inner).consume(amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Passa os pedacos pelo filtro e devolve a saida crua.
    ///
    /// Recebe pedacos separados de proposito: e' na fronteira entre eles que
    /// mora o bug de UTF-8 que estes testes protegem.
    async fn run_filter(chunks: &[&[u8]], kind: DatabaseType, options: &RestoreOptions) -> Vec<u8> {
        let Some(filter) = LineFilter::build(kind, options) else {
            return chunks.concat();
        };

        let mut writer = FilterWriter::new(Vec::new(), filter);
        for chunk in chunks {
            writer.write_all(chunk).await.expect("escreve o bloco");
        }
        writer.shutdown().await.expect("fecha o filtro");

        writer.into_inner()
    }

    fn options(mode: RestoreMode) -> RestoreOptions {
        RestoreOptions {
            mode,
            ..RestoreOptions::default()
        }
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("saida em UTF-8")
    }

    #[tokio::test]
    async fn a_full_restore_installs_no_filter_at_all() {
        // Sem filtro nao ha' quebra em linhas, e some o teto de memoria por
        // linha — que e' a saida indicada quando o dump tem uma linha enorme.
        assert!(LineFilter::build(DatabaseType::Mysql, &options(RestoreMode::Full)).is_none());
        assert!(LineFilter::build(DatabaseType::Postgresql, &options(RestoreMode::Full)).is_none());
    }

    #[tokio::test]
    async fn does_not_corrupt_a_multibyte_character_split_between_chunks() {
        // O bug que o `StringDecoder` do Adonis resolve. Aqui os filtros
        // trabalham em bytes, e `\n` nunca aparece dentro de uma sequencia
        // UTF-8 — a fronteira simplesmente nao existe.
        let sql = "CREATE TABLE informação (id int);\nGRANT ALL ON informação TO joão;\n";
        let full = sql.as_bytes();
        let split = full
            .windows(2)
            .position(|pair| pair == "ç".as_bytes())
            .expect("fixture: caractere multibyte ausente")
            + 1;

        let output = run_filter(
            &[&full[..split], &full[split..]],
            DatabaseType::Postgresql,
            &RestoreOptions {
                no_privileges: true,
                ..RestoreOptions::default()
            },
        )
        .await;

        let restored = text(&output);
        assert!(
            !restored.contains('\u{FFFD}'),
            "corrompeu UTF-8: {restored}"
        );
        assert!(restored.contains("CREATE TABLE informação (id int);"));
        assert!(!restored.contains("GRANT ALL"));
    }

    #[tokio::test]
    async fn survives_a_cut_every_three_bytes() {
        let sql = "INSERT INTO ação VALUES ('coração', 'não', 'informação');\n";
        let full = sql.as_bytes();
        let chunks: Vec<&[u8]> = full.chunks(3).collect();

        let output = run_filter(
            &chunks,
            DatabaseType::Postgresql,
            &RestoreOptions {
                no_comments: true,
                ..RestoreOptions::default()
            },
        )
        .await;

        let restored = text(&output);
        assert!(!restored.contains('\u{FFFD}'));
        assert!(restored.contains("coração"));
        assert!(restored.contains("informação"));
    }

    #[tokio::test]
    async fn postgres_schema_only_drops_copy_blocks_and_inserts() {
        let sql = concat!(
            "CREATE TABLE endereço (id int, descrição text);\n",
            "COPY endereço (id, descrição) FROM stdin;\n",
            "1\tsão paulo\n",
            "\\.\n",
            "INSERT INTO endereço VALUES (2, 'brasília');\n",
            "CREATE INDEX idx ON endereço (id);\n",
        );

        let output = run_filter(
            &[sql.as_bytes()],
            DatabaseType::Postgresql,
            &options(RestoreMode::SchemaOnly),
        )
        .await;

        let restored = text(&output);
        assert!(restored.contains("CREATE TABLE endereço"));
        assert!(restored.contains("CREATE INDEX idx ON endereço (id);"));
        assert!(!restored.contains("são paulo"));
        assert!(!restored.contains("brasília"));
    }

    #[tokio::test]
    async fn a_copy_block_ends_only_at_its_terminator() {
        // Sem o estado, a primeira linha de dados que nao casasse com nenhum
        // padrao voltaria a passar — e os dados vazariam para um schema-only.
        let sql = concat!(
            "COPY t (a) FROM stdin;\n",
            "linha de dados qualquer\n",
            "CREATE TABLE dentro_do_bloco (id int);\n",
            "\\.\n",
            "CREATE TABLE depois (id int);\n",
        );

        let output = run_filter(
            &[sql.as_bytes()],
            DatabaseType::Postgresql,
            &options(RestoreMode::SchemaOnly),
        )
        .await;

        let restored = text(&output);
        assert!(!restored.contains("dentro_do_bloco"));
        assert!(restored.contains("CREATE TABLE depois"));
    }

    #[tokio::test]
    async fn mysql_data_only_keeps_accented_inserts() {
        let sql = concat!(
            "CREATE TABLE usuário (id int);\n",
            "INSERT INTO usuário VALUES (1, 'josé');\n",
            "LOCK TABLES `usuário` WRITE;\n",
            "UNLOCK TABLES;\n",
        );

        let output = run_filter(
            &[sql.as_bytes()],
            DatabaseType::Mysql,
            &options(RestoreMode::DataOnly),
        )
        .await;

        let restored = text(&output);
        assert!(restored.contains("INSERT INTO usuário VALUES (1, 'josé');"));
        // O `LOCK`/`UNLOCK` acompanha os dados; o DDL nao.
        assert!(restored.contains("LOCK TABLES"));
        assert!(!restored.contains("CREATE TABLE"));
    }

    #[tokio::test]
    async fn mysql_schema_only_drops_inserts_and_locks() {
        let sql = concat!(
            "CREATE TABLE t (id int);\n",
            "LOCK TABLES `t` WRITE;\n",
            "INSERT INTO t VALUES (1);\n",
            "UNLOCK TABLES;\n",
        );

        let output = run_filter(
            &[sql.as_bytes()],
            DatabaseType::Mysql,
            &options(RestoreMode::SchemaOnly),
        )
        .await;

        let restored = text(&output);
        assert_eq!(restored, "CREATE TABLE t (id int);\n");
    }

    #[tokio::test]
    async fn postgres_data_only_keeps_the_trigger_toggles() {
        // O `COPY` exige os triggers desligados: sem estas linhas, restaurar
        // dados falha em toda tabela com chave estrangeira.
        let sql = concat!(
            "ALTER TABLE public.pedidos DISABLE TRIGGER ALL;\n",
            "COPY public.pedidos (id) FROM stdin;\n",
            "1\n",
            "\\.\n",
            "ALTER TABLE public.pedidos ENABLE TRIGGER ALL;\n",
            "CREATE INDEX i ON public.pedidos (id);\n",
        );

        let output = run_filter(
            &[sql.as_bytes()],
            DatabaseType::Postgresql,
            &options(RestoreMode::DataOnly),
        )
        .await;

        let restored = text(&output);
        assert!(restored.contains("DISABLE TRIGGER"));
        assert!(restored.contains("ENABLE TRIGGER"));
        assert!(!restored.contains("CREATE INDEX"));
    }

    #[tokio::test]
    async fn the_drop_options_apply_on_top_of_the_mode() {
        let sql = concat!(
            "ALTER TABLE t OWNER TO postgres;\n",
            "GRANT ALL ON t TO app;\n",
            "SET default_tablespace = 'x';\n",
            "COMMENT ON TABLE t IS 'oi';\n",
            "CREATE TABLE t (id int);\n",
        );

        let output = run_filter(
            &[sql.as_bytes()],
            DatabaseType::Postgresql,
            &RestoreOptions {
                no_owner: true,
                no_privileges: true,
                no_tablespaces: true,
                no_comments: true,
                ..RestoreOptions::default()
            },
        )
        .await;

        assert_eq!(text(&output), "CREATE TABLE t (id int);\n");
    }

    #[tokio::test]
    async fn no_create_db_only_applies_to_mysql() {
        let sql = "CREATE DATABASE app;\nUSE `app`;\nCREATE TABLE t (id int);\n";
        let with_flag = RestoreOptions {
            no_create_db: true,
            ..RestoreOptions::default()
        };

        let mysql = run_filter(&[sql.as_bytes()], DatabaseType::Mysql, &with_flag).await;
        assert_eq!(text(&mysql), "CREATE TABLE t (id int);\n");

        // No PostgreSQL a opcao nao existe: nenhum filtro e' montado.
        assert!(LineFilter::build(DatabaseType::Postgresql, &with_flag).is_none());
    }

    #[tokio::test]
    async fn a_dump_without_a_final_newline_keeps_its_last_line() {
        let sql = "CREATE TABLE t (id int);\nCREATE INDEX i ON t (id);";

        let output = run_filter(
            &[sql.as_bytes()],
            DatabaseType::Mysql,
            &options(RestoreMode::SchemaOnly),
        )
        .await;

        assert!(text(&output).ends_with("CREATE INDEX i ON t (id);\n"));
    }

    #[tokio::test]
    async fn the_trailing_remainder_is_filtered_in_schema_only() {
        let sql = "CREATE TABLE t (id int);\nINSERT INTO t VALUES (1);";

        let output = run_filter(
            &[sql.as_bytes()],
            DatabaseType::Mysql,
            &options(RestoreMode::SchemaOnly),
        )
        .await;

        assert_eq!(text(&output), "CREATE TABLE t (id int);\n");
    }

    #[tokio::test]
    async fn refuses_a_line_that_never_ends() {
        // Sem teto, um dump numa linha so' cresceria em memoria ate' derrubar o
        // processo — e a mensagem precisa dizer qual e' a saida.
        let Some(filter) = LineFilter::build(DatabaseType::Mysql, &options(RestoreMode::DataOnly))
        else {
            panic!("data-only sempre monta filtro")
        };

        let mut writer = FilterWriter::new(Vec::new(), filter).with_max_line_bytes(16);
        let error = writer
            .write_all(&[b'x'; 64])
            .await
            .expect_err("devia recusar");

        assert!(error.to_string().contains("full"), "mensagem: {error}");
    }

    #[test]
    fn postgres_stops_at_the_first_error() {
        // Sem `ON_ERROR_STOP=1` o `psql` segue adiante e sai com codigo 0: a
        // restauracao apareceria como concluida com o banco pela metade.
        let command = build_command(&target(DatabaseType::Postgresql), "vendas");

        assert_eq!(command.program, "psql");
        assert!(command.args.contains(&"ON_ERROR_STOP=1".to_string()));
        assert_eq!(command.env, vec![("PGPASSWORD", "s3nh4".to_string())]);
    }

    #[test]
    fn mysql_receives_the_database_as_the_last_argument() {
        let command = build_command(&target(DatabaseType::Mysql), "vendas");

        assert_eq!(command.program, "mysql");
        assert_eq!(command.args.last().map(String::as_str), Some("vendas"));
        assert!(command.args.contains(&"--skip-ssl".to_string()));
    }

    fn target(kind: DatabaseType) -> DatabaseTarget {
        DatabaseTarget {
            kind,
            host: "db.local".to_string(),
            port: 5432,
            username: "root".to_string(),
            password: "s3nh4".to_string(),
            database: None,
            ssl: false,
        }
    }

    #[test]
    fn separates_warnings_from_the_rest_of_stderr() {
        let warnings = extract_warnings(
            "psql:dump.sql:1: NOTICE:  table \"t\" does not exist\n\
             ERROR:  syntax error\n\
             mysqldump: [Warning] Using a password on the command line\n",
        );

        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|line| !line.contains("syntax error")));
    }

    #[test]
    fn an_empty_stderr_has_no_warnings() {
        assert!(extract_warnings("").is_empty());
    }

    #[test]
    fn the_modes_round_trip_through_their_wire_names() {
        for mode in [
            RestoreMode::Full,
            RestoreMode::SchemaOnly,
            RestoreMode::DataOnly,
        ] {
            assert_eq!(mode.as_str().parse::<RestoreMode>(), Ok(mode));
        }
        assert!("partial".parse::<RestoreMode>().is_err());
        assert_eq!(RestoreMode::CHOICES.len(), 3);
    }
}
