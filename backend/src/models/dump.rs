//! Pipeline de dump — `mysqldump`/`pg_dump` → sha256 → gzip → disco.
//!
//! Porte de `executeDumpProcess` do
//! `app/services/backup_service.ts`.
//!
//! ## Nada e' bufferizado
//!
//! O `stdout` do dump entra num [`tokio::io::copy`] e sai no arquivo. Entre os
//! dois passam o hash e o gzip, os dois como **adaptadores de escrita**, nao
//! como colecoes. Um `read_to_end` seria uma linha mais curto e faria o pico de
//! memoria acompanhar o tamanho do banco: um dump de 40 GB derrubaria o
//! processo, e so' no cliente maior.
//!
//! O backpressure vem de graca dessa forma: quando o disco fica para tras, a
//! escrita bloqueia, a leitura para, e o pipe do dump enche — o `mysqldump`
//! desacelera sozinho.
//!
//! ## O checksum e' dos bytes **descomprimidos**
//!
//! [`HashingWriter`] fica **antes** do gzip. E' o que o Adonis faz, e nao e'
//! arbitrario: o gzip carrega timestamp no cabecalho, entao comprimir o mesmo
//! dump duas vezes produz arquivos diferentes. Um checksum do arquivo `.gz`
//! nunca bateria entre duas execucoes e seria inutil para verificar integridade.
//!
//! ## A senha nao entra na linha de comando do PostgreSQL
//!
//! `pg_dump` recebe a senha por `PGPASSWORD` no ambiente; `mysqldump` a recebe
//! em `--password=`, que aparece no `ps` da maquina. E' o que o Adonis faz, e
//! trocar por `MYSQL_PWD` mudaria o comportamento observavel de um jeito que
//! nenhum teste de contrato cobre — fica registrado como divergencia
//! **deliberadamente nao feita**, para a revisao de seguranca da 12.8 decidir.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::process::Command;

use crate::models::connections::DatabaseType;
use crate::models::database_driver::DatabaseTarget;
use crate::models::process_output::{drain, DEFAULT_LIMIT_BYTES};

/// Valor especial de `database_name` que pede o dump de **todos** os bancos.
pub const ALL_DATABASES_MARKER: &str = "*";

/// Nome de arquivo usado quando o dump e' de todos os bancos.
const ALL_DATABASES_PREFIX: &str = "all_databases";

/// Comando montado, pronto para virar processo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpCommand {
    pub program: &'static str,
    pub args: Vec<String>,
    /// Variaveis acrescentadas ao ambiente herdado.
    pub env: Vec<(&'static str, String)>,
}

/// Desfecho de um dump bem-sucedido.
#[derive(Debug, Clone)]
pub struct DumpOutcome {
    /// Caminho relativo gravado em `backups.file_path`.
    pub file_path: String,
    /// Caminho absoluto no disco desta maquina.
    pub local_full_path: PathBuf,
    pub file_name: String,
    pub file_size: i64,
    /// SHA-256 hex dos bytes **antes** da compressao.
    pub checksum: String,
}

/// Por que um dump falhou.
///
/// Separado em variantes em vez de uma `String` porque o chamador precisa
/// distinguir o que grava em `backups.exit_code`: um binario ausente nao tem
/// codigo de saida, e gravar `0` ali seria indistinguivel de sucesso.
#[derive(Debug, thiserror::Error)]
pub enum DumpError {
    #[error(
        "Falha ao executar {program}: {source}. Verifique se o binário está instalado e no PATH."
    )]
    Spawn {
        program: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("Erro ao gravar o arquivo de backup: {0}")]
    Write(#[source] std::io::Error),
    #[error("{message}")]
    Failed {
        message: String,
        exit_code: Option<i64>,
    },
}

impl DumpError {
    /// Codigo de saida a gravar em `backups.exit_code`, quando houver.
    #[must_use]
    pub const fn exit_code(&self) -> Option<i64> {
        match self {
            Self::Failed { exit_code, .. } => *exit_code,
            _ => None,
        }
    }
}

/// Nome, caminho relativo e caminho absoluto de um dump.
///
/// Separado da execucao para ser testavel: o nome carrega o horario, e um teste
/// que precisasse levantar um `mysqldump` para conferir o formato do nome nao
/// rodaria em CI.
#[must_use]
pub fn build_file_paths(
    base: &Path,
    connection_id: i64,
    database_name: &str,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> (String, String, PathBuf) {
    let prefix = if database_name == ALL_DATABASES_MARKER {
        ALL_DATABASES_PREFIX
    } else {
        database_name
    };

    let file_name = format!("{prefix}_{}.sql.gz", now.format("%Y%m%d_%H%M%S"));
    // Sempre com `/`: o mesmo valor vai virar chave de objeto num bucket na
    // Fase 8, e `\` ali produziria um "diretorio" com barra invertida no nome.
    let relative = format!("{connection_id}/{file_name}");
    let full = base.join(connection_id.to_string()).join(&file_name);

    (file_name, relative, full)
}

/// Monta a linha de comando do dump.
#[must_use]
pub fn build_command(target: &DatabaseTarget, database_name: &str) -> DumpCommand {
    let all_databases = database_name == ALL_DATABASES_MARKER;

    match target.kind {
        DatabaseType::Postgresql => build_postgres_command(target, database_name, all_databases),
        DatabaseType::Mysql | DatabaseType::Mariadb => {
            build_mysql_command(target, database_name, all_databases)
        }
    }
}

/// `pg_dump` para um banco, `pg_dumpall` para todos.
///
/// `pg_dumpall` nao aceita `-d`: ele varre o cluster inteiro, incluindo roles e
/// tablespaces. Passar o nome do banco ali seria um erro de sintaxe do proprio
/// comando.
fn build_postgres_command(
    target: &DatabaseTarget,
    database_name: &str,
    all_databases: bool,
) -> DumpCommand {
    let mut args = vec![
        "-h".to_string(),
        target.host.clone(),
        "-p".to_string(),
        target.port.to_string(),
        "-U".to_string(),
        target.username.clone(),
    ];

    if !all_databases {
        args.push("-d".to_string());
        args.push(database_name.to_string());
    }

    // Sem isto o `pg_dump` abre um prompt de senha e o processo trava para
    // sempre esperando um terminal que nao existe.
    args.push("--no-password".to_string());

    DumpCommand {
        program: if all_databases {
            "pg_dumpall"
        } else {
            "pg_dump"
        },
        args,
        env: vec![("PGPASSWORD", target.password.clone())],
    }
}

fn build_mysql_command(
    target: &DatabaseTarget,
    database_name: &str,
    all_databases: bool,
) -> DumpCommand {
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

    // `--single-transaction` tira o dump de um snapshot consistente sem
    // travar as tabelas — sem ele, um backup de banco em uso bloqueia a
    // aplicacao do cliente pelo tempo inteiro do dump.
    args.push("--single-transaction".to_string());
    args.push("--routines".to_string());
    args.push("--triggers".to_string());

    if all_databases {
        args.push("--all-databases".to_string());
    } else {
        args.push(database_name.to_string());
    }

    DumpCommand {
        program: "mysqldump",
        args,
        env: Vec::new(),
    }
}

/// Faz o dump de um banco e devolve tudo o que o registro precisa.
///
/// E' o unico ponto que o controller chama: monta os caminhos, monta o comando,
/// executa e embrulha o desfecho. Manter as tres etapas separadas abaixo e' o
/// que permite testar o nome do arquivo e a linha de comando sem levantar
/// processo nenhum.
pub async fn execute<F>(
    target: &DatabaseTarget,
    base: &Path,
    connection_id: i64,
    database_name: &str,
    now: chrono::DateTime<chrono::FixedOffset>,
    on_progress: F,
) -> std::result::Result<DumpOutcome, DumpError>
where
    F: FnMut(u64) + Send,
{
    let (file_name, file_path, local_full_path) =
        build_file_paths(base, connection_id, database_name, now);
    let command = build_command(target, database_name);

    let (file_size, checksum) = run(&command, &local_full_path, on_progress).await?;

    Ok(DumpOutcome {
        file_path,
        local_full_path,
        file_name,
        file_size,
        checksum,
    })
}

/// Executa o dump, gravando o `.sql.gz` em `full_path`.
///
/// `on_progress` recebe o total de bytes **comprimidos** ja' escritos, como no
/// Adonis. E' chamado a cada bloco; o estrangulamento por tempo fica no
/// emissor, nao aqui — misturar as duas coisas obrigaria o pipeline a conhecer
/// o SSE.
pub async fn run<F>(
    command: &DumpCommand,
    full_path: &Path,
    mut on_progress: F,
) -> std::result::Result<(i64, String), DumpError>
where
    F: FnMut(u64) + Send,
{
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(DumpError::Write)?;
    }

    let mut child = Command::new(command.program)
        .args(&command.args)
        .envs(command.env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // Sem isto, o filho continua vivo se a task for cancelada — um
        // `mysqldump` orfao segurando uma transacao no servidor do cliente.
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| DumpError::Spawn {
            program: command.program,
            source,
        })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        DumpError::Write(std::io::Error::other("o processo de dump não expôs stdout"))
    })?;
    let stderr = child.stderr.take();

    // O stderr e' drenado **em paralelo** com o stdout. Le-lo depois deixaria o
    // dump travar assim que o pipe de erro enchesse — o que acontece com
    // qualquer `mysqldump` que emita muitos avisos.
    let stderr_task = tokio::spawn(async move {
        match stderr {
            Some(pipe) => drain(pipe, DEFAULT_LIMIT_BYTES).await.to_text(),
            None => String::new(),
        }
    });

    let write_result = write_dump(stdout, full_path, &mut on_progress).await;

    let status = child.wait().await.map_err(DumpError::Write)?;
    let stderr_text = stderr_task.await.unwrap_or_default();
    let exit_code = status.code().map(i64::from);

    // O desfecho do processo tem precedencia sobre o erro de escrita: quando o
    // dump aborta no meio (credencial errada, tabela sumindo), o stdout fecha e
    // a escrita pode falhar por consequencia. O stderr do banco explica a causa
    // real; o erro de I/O so' descreveria o sintoma.
    if !status.success() {
        let message = if stderr_text.trim().is_empty() {
            format!("Processo terminou com código {}", format_exit(exit_code))
        } else {
            stderr_text
        };

        return Err(DumpError::Failed { message, exit_code });
    }

    let (bytes_written, checksum) = write_result?;

    // Reportar sucesso com um arquivo truncado seria pior que falhar: o
    // operador so' descobriria na hora de restaurar.
    let file_size = i64::try_from(bytes_written).unwrap_or(i64::MAX);

    Ok((file_size, checksum))
}

fn format_exit(exit_code: Option<i64>) -> String {
    exit_code.map_or_else(|| "desconhecido".to_string(), |code| code.to_string())
}

/// Encadeia stdout → hash → gzip → arquivo e devolve (bytes gravados, checksum).
async fn write_dump<R, F>(
    mut source: R,
    full_path: &Path,
    on_progress: &mut F,
) -> std::result::Result<(u64, String), DumpError>
where
    R: tokio::io::AsyncRead + Unpin,
    F: FnMut(u64) + Send,
{
    let file = tokio::fs::File::create(full_path)
        .await
        .map_err(DumpError::Write)?;

    let counting = CountingWriter::new(file);
    let gzip = async_compression::tokio::write::GzipEncoder::new(counting);
    let mut hashing = HashingWriter::new(gzip);

    let copy = tokio::io::copy(&mut source, &mut hashing).await;

    // `shutdown` e' o que fecha o quadro final do gzip. Sem ele o arquivo fica
    // sem o trailer e nenhum `gunzip` consegue abri-lo — inclusive o nosso
    // restore.
    let flush = hashing.shutdown().await;

    let checksum = hashing.finish_hex();
    let mut gzip = hashing.into_inner();
    let bytes_written = gzip.get_mut().bytes_written();
    on_progress(bytes_written);

    copy.map_err(DumpError::Write)?;
    flush.map_err(DumpError::Write)?;

    Ok((bytes_written, checksum))
}

/// Escritor que alimenta um SHA-256 com tudo o que passa por ele.
struct HashingWriter<W> {
    inner: W,
    hasher: Sha256,
}

impl<W> HashingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    fn finish_hex(&self) -> String {
        hex::encode(self.hasher.clone().finalize())
    }

    fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for HashingWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let written = std::task::ready!(Pin::new(&mut this.inner).poll_write(cx, buf))?;

        // So' o que foi de fato aceito entra no hash: um `poll_write` parcial e'
        // legitimo, e alimentar o buffer inteiro produziria um checksum de
        // bytes que nunca chegaram ao arquivo.
        this.hasher.update(&buf[..written]);

        Poll::Ready(Ok(written))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// Escritor que conta os bytes que passam — o tamanho comprimido do arquivo.
///
/// Contar aqui, e nao com um `stat` no final, evita uma ida ao sistema de
/// arquivos e da' o numero mesmo quando o destino nao for um arquivo (Fase 8).
struct CountingWriter<W> {
    inner: W,
    bytes: u64,
}

impl<W> CountingWriter<W> {
    const fn new(inner: W) -> Self {
        Self { inner, bytes: 0 }
    }

    const fn bytes_written(&self) -> u64 {
        self.bytes
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for CountingWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let written = std::task::ready!(Pin::new(&mut this.inner).poll_write(cx, buf))?;
        this.bytes += written as u64;

        Poll::Ready(Ok(written))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: DatabaseType) -> DatabaseTarget {
        DatabaseTarget {
            kind,
            host: "db.local".to_string(),
            port: 3306,
            username: "root".to_string(),
            password: "s3nh4".to_string(),
            database: None,
            ssl: false,
        }
    }

    #[test]
    fn mysql_dumps_one_database_with_a_consistent_snapshot() {
        let command = build_command(&target(DatabaseType::Mysql), "vendas");

        assert_eq!(command.program, "mysqldump");
        assert!(command.args.contains(&"--single-transaction".to_string()));
        assert!(command.args.contains(&"--routines".to_string()));
        assert!(command.args.contains(&"--triggers".to_string()));
        // O nome do banco e' o ultimo argumento, como no Adonis.
        assert_eq!(command.args.last().map(String::as_str), Some("vendas"));
    }

    #[test]
    fn mariadb_uses_the_same_client_as_mysql() {
        assert_eq!(
            build_command(&target(DatabaseType::Mariadb), "vendas").program,
            "mysqldump"
        );
    }

    #[test]
    fn mysql_disables_ssl_unless_it_was_asked_for() {
        // Exigir TLS por padrao derrubaria toda conexao com banco interno sem
        // certificado — que e' o caso comum.
        let plain = build_command(&target(DatabaseType::Mysql), "vendas");
        assert!(plain.args.contains(&"--skip-ssl".to_string()));

        let mut secure = target(DatabaseType::Mysql);
        secure.ssl = true;
        let secure = build_command(&secure, "vendas");
        assert!(!secure.args.contains(&"--skip-ssl".to_string()));
    }

    #[test]
    fn mysql_switches_to_all_databases_for_the_marker() {
        let command = build_command(&target(DatabaseType::Mysql), ALL_DATABASES_MARKER);

        assert!(command.args.contains(&"--all-databases".to_string()));
        assert!(!command.args.contains(&ALL_DATABASES_MARKER.to_string()));
    }

    #[test]
    fn postgres_passes_the_password_through_the_environment() {
        // Em `--password=` ela apareceria no `ps` de qualquer usuario da
        // maquina; o `pg_dump` aceita a variavel, e o Adonis usa a variavel.
        let command = build_command(&target(DatabaseType::Postgresql), "vendas");

        assert_eq!(command.program, "pg_dump");
        assert_eq!(command.env, vec![("PGPASSWORD", "s3nh4".to_string())]);
        assert!(!command.args.iter().any(|arg| arg.contains("s3nh4")));
    }

    #[test]
    fn postgres_never_prompts_for_a_password() {
        // Sem `--no-password` o processo trava esperando um terminal.
        let command = build_command(&target(DatabaseType::Postgresql), "vendas");
        assert!(command.args.contains(&"--no-password".to_string()));
    }

    #[test]
    fn postgres_full_backup_uses_dumpall_without_a_database() {
        let command = build_command(&target(DatabaseType::Postgresql), ALL_DATABASES_MARKER);

        assert_eq!(command.program, "pg_dumpall");
        // `pg_dumpall` nao aceita `-d`: passar seria erro de sintaxe.
        assert!(!command.args.contains(&"-d".to_string()));
    }

    #[test]
    fn builds_the_file_name_with_the_timestamp() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 8, 9)
            .and_then(|date| date.and_hms_opt(14, 3, 7))
            .expect("data de teste")
            .and_utc()
            .fixed_offset();

        let (name, relative, full) = build_file_paths(Path::new("/backups"), 12, "vendas", now);

        assert_eq!(name, "vendas_20260809_140307.sql.gz");
        assert_eq!(relative, "12/vendas_20260809_140307.sql.gz");
        assert_eq!(full, Path::new("/backups").join("12").join(&name));
    }

    #[test]
    fn the_relative_path_always_uses_forward_slashes() {
        // Vira chave de objeto num bucket na Fase 8; `\` produziria um nome
        // com barra invertida em vez de um prefixo.
        let now = chrono::NaiveDate::from_ymd_opt(2026, 8, 9)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .expect("data de teste")
            .and_utc()
            .fixed_offset();

        let (_, relative, _) = build_file_paths(Path::new("C:\\backups"), 3, "vendas", now);
        assert!(!relative.contains('\\'), "caminho relativo: {relative}");
    }

    #[test]
    fn a_full_backup_gets_a_descriptive_file_name() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 8, 9)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .expect("data de teste")
            .and_utc()
            .fixed_offset();

        let (name, _, _) = build_file_paths(Path::new("/backups"), 1, ALL_DATABASES_MARKER, now);

        // `*_20260809_000000.sql.gz` seria um nome invalido em boa parte dos
        // sistemas de arquivos, e uma chave estranha num bucket.
        assert!(name.starts_with("all_databases_"), "nome: {name}");
    }

    #[tokio::test]
    async fn compresses_while_hashing_the_uncompressed_bytes() {
        let dir = tempfile::tempdir().expect("diretorio temporario");
        let path = dir.path().join("saida.sql.gz");
        let payload = b"CREATE TABLE clientes (id INT);\n".repeat(512);

        let mut seen = 0_u64;
        let (bytes, checksum) = write_dump(&payload[..], &path, &mut |written| seen = written)
            .await
            .expect("grava o dump");

        // O checksum e' do conteudo cru, nao do `.gz`.
        let expected = hex::encode(Sha256::digest(&payload));
        assert_eq!(checksum, expected);

        // O arquivo e' menor que a origem — o gzip rodou de verdade.
        let on_disk = tokio::fs::metadata(&path).await.expect("arquivo criado");
        assert_eq!(on_disk.len(), bytes);
        assert!(bytes < payload.len() as u64, "nao comprimiu: {bytes} bytes");
        assert_eq!(seen, bytes);
    }

    #[tokio::test]
    async fn writes_a_gzip_file_that_can_be_read_back() {
        // Faltar o `shutdown` produziria um arquivo sem trailer: o tamanho
        // pareceria certo e nenhum `gunzip` conseguiria abri-lo.
        use tokio::io::AsyncReadExt;

        let dir = tempfile::tempdir().expect("diretorio temporario");
        let path = dir.path().join("saida.sql.gz");
        let payload = b"SELECT 1;\n".repeat(200);

        write_dump(&payload[..], &path, &mut |_| {})
            .await
            .expect("grava o dump");

        let file = tokio::fs::File::open(&path).await.expect("abre o arquivo");
        let mut decoder =
            async_compression::tokio::bufread::GzipDecoder::new(tokio::io::BufReader::new(file));
        let mut restored = Vec::new();
        decoder
            .read_to_end(&mut restored)
            .await
            .expect("descomprime");

        assert_eq!(restored, payload);
    }

    #[tokio::test]
    async fn reports_a_missing_binary_instead_of_a_silent_failure() {
        let dir = tempfile::tempdir().expect("diretorio temporario");
        let command = DumpCommand {
            program: "mysqldump",
            args: vec!["--version".to_string()],
            env: Vec::new(),
        };

        // O binario pode existir na maquina do desenvolvedor; o teste so'
        // exige que, quando falta, a mensagem diga o que fazer.
        if let Err(error) = run(&command, &dir.path().join("x.sql.gz"), |_| {}).await {
            let message = error.to_string();
            assert!(
                message.contains("mysqldump"),
                "a mensagem precisa nomear o binário: {message}"
            );
        }
    }

    #[test]
    fn only_a_process_failure_carries_an_exit_code() {
        // Gravar `0` em `exit_code` para um binario ausente seria
        // indistinguivel de sucesso na tabela de backups.
        let spawn = DumpError::Spawn {
            program: "pg_dump",
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };
        assert_eq!(spawn.exit_code(), None);

        let failed = DumpError::Failed {
            message: "access denied".to_string(),
            exit_code: Some(2),
        };
        assert_eq!(failed.exit_code(), Some(2));
    }
}
