//! Importacao de um arquivo de backup externo (tarefa 7.7 do roadmap).
//!
//! Porte de `app/services/backup_import_service.ts`.
//!
//! ## A extensao e' a primeira barreira, os magic bytes sao a segunda
//!
//! A extensao decide se o arquivo entra; o conteudo decide se ele e' o que diz
//! ser. A ordem importa: recusar `.exe` **antes** de gravar qualquer byte e' o
//! que impede que a area de backups vire um diretorio de upload arbitrario. A
//! verificacao de conteudo e' opcional (`verifyIntegrity`) porque um dump de
//! 400 GB nao pode ser lido inteiro so' para confirmar que comeca com `CREATE`.
//!
//! ## O nome do arquivo entra no caminho, entao e' saneado
//!
//! `import_<epoch>_<nome original>` vai para o disco e para `backups.file_path`.
//! Um nome com `../` ou com separador de diretorio escreveria fora da area de
//! backups — e depois o `download` leria de la'. [`sanitize_file_name`] e' lista
//! de permissao pelo mesmo motivo de `quote_identifier` no driver: uma lista de
//! bloqueio erra em silencio no dia em que aparece um caractere novo.

use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

/// Formatos aceitos, ja' canonizados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportedFormat {
    Sql,
    #[serde(rename = "sql.gz")]
    SqlGz,
    Dump,
    Zip,
    Tar,
}

impl ImportedFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sql => "sql",
            Self::SqlGz => "sql.gz",
            Self::Dump => "dump",
            Self::Zip => "zip",
            Self::Tar => "tar",
        }
    }

    /// So' o `.sql.gz` e' descomprimido pelo restore.
    ///
    /// Um `.dump` do `pg_dump -Fc` tambem e' comprimido, mas por dentro: passar
    /// um gunzip nele produziria lixo.
    #[must_use]
    pub const fn is_gzip_wrapped(self) -> bool {
        matches!(self, Self::SqlGz)
    }
}

/// Texto das extensoes aceitas, usado nas mensagens de erro.
pub const ACCEPTED_EXTENSIONS: &str =
    ".sql, .sql.gz, .gz, .dump, .pgdump, .zip, .tar, .tar.gz, .tgz";

/// Teto de tamanho do upload, igual ao `size: '500mb'` do Adonis.
pub const MAX_UPLOAD_BYTES: u64 = 500 * 1024 * 1024;

/// Magic bytes.
const MAGIC_GZIP: &[u8] = &[0x1f, 0x8b];
const MAGIC_ZIP: &[u8] = &[0x50, 0x4b, 0x03, 0x04];
const MAGIC_PGDUMP: &[u8] = b"PGDMP";
/// `ustar` comeca no offset 257 de um TAR POSIX/GNU.
const TAR_MAGIC_OFFSET: usize = 257;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("Formato de arquivo não suportado. Formatos aceitos: {ACCEPTED_EXTENSIONS}")]
    UnsupportedExtension,
    #[error("O arquivo tem extensão .gz mas não é um arquivo Gzip válido")]
    NotGzip,
    #[error("Nome de arquivo inválido")]
    InvalidFileName,
    #[error("Falha na verificação de integridade: {0}")]
    Integrity(String),
    #[error("Erro ao gravar o arquivo importado: {0}")]
    Io(#[from] std::io::Error),
}

/// Resultado de uma verificacao de integridade.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityResult {
    pub valid: bool,
    pub message: String,
    /// Omitido quando nao ha' aviso — o Adonis nao emite a chave vazia.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

impl IntegrityResult {
    fn valid(message: impl Into<String>) -> Self {
        Self {
            valid: true,
            message: message.into(),
            warnings: None,
        }
    }

    fn valid_with_warning(message: impl Into<String>, warning: impl Into<String>) -> Self {
        Self {
            valid: true,
            message: message.into(),
            warnings: Some(vec![warning.into()]),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self {
            valid: false,
            message: message.into(),
            warnings: None,
        }
    }
}

/// Aviso comum aos arquivos que o restore nao sabe abrir sozinho.
const NEEDS_EXTRACTION: &str =
    "Arquivos importados neste formato requerem extração prévia para serem restaurados pela ferramenta de restore";

/// Extensao canonica de um nome de arquivo.
///
/// As duplas (`.sql.gz`, `.tar.gz`) sao testadas primeiro: `extname()` devolve
/// so' `.gz`, e cair nesse ramo trataria um `.tar.gz` como SQL comprimido.
#[must_use]
pub fn resolve_extension(file_name: &str) -> String {
    let lower = file_name.to_lowercase();

    if lower.ends_with(".sql.gz") {
        return ".sql.gz".to_string();
    }
    if lower.ends_with(".tar.gz") {
        return ".tar.gz".to_string();
    }

    lower
        .rfind('.')
        .map(|position| lower[position..].to_string())
        .unwrap_or_default()
}

/// Formato correspondente a uma extensao, sem olhar o conteudo.
#[must_use]
pub fn format_for_extension(extension: &str) -> Option<ImportedFormat> {
    Some(match extension {
        ".sql" => ImportedFormat::Sql,
        ".sql.gz" | ".gz" => ImportedFormat::SqlGz,
        ".dump" | ".pgdump" | ".pg_dump" => ImportedFormat::Dump,
        ".zip" => ImportedFormat::Zip,
        ".tar" | ".tar.gz" | ".tgz" => ImportedFormat::Tar,
        _ => return None,
    })
}

/// Decide o formato pelo nome, confirmando com os magic bytes quando a extensao
/// e' ambigua.
///
/// `.gz` sozinho nao diz se e' SQL ou outra coisa; o Adonis confere o cabecalho
/// antes de aceitar, e uma extensao mentirosa aqui viraria um `gunzip` que falha
/// so' na hora da restauracao.
pub fn detect_format(
    file_name: &str,
    header: &[u8],
) -> std::result::Result<ImportedFormat, ImportError> {
    let extension = resolve_extension(file_name);
    let format = format_for_extension(&extension).ok_or(ImportError::UnsupportedExtension)?;

    if extension == ".gz" && !header.starts_with(MAGIC_GZIP) {
        return Err(ImportError::NotGzip);
    }

    Ok(format)
}

/// Confere o conteudo contra o formato declarado.
///
/// Recebe o cabecalho ja' lido em vez do caminho: assim o mesmo codigo serve ao
/// upload em andamento e a um arquivo em disco, e o teste nao precisa de
/// arquivo nenhum.
#[must_use]
pub fn verify_integrity(format: ImportedFormat, header: &[u8]) -> IntegrityResult {
    match format {
        ImportedFormat::Sql => verify_sql(header),
        ImportedFormat::SqlGz => verify_gzip(header),
        ImportedFormat::Dump => verify_pgdump(header),
        ImportedFormat::Zip => verify_zip(header),
        ImportedFormat::Tar => verify_tar(header),
    }
}

/// Instrucoes que identificam um dump SQL de verdade.
static SQL_PATTERN: std::sync::LazyLock<regex::bytes::Regex> = std::sync::LazyLock::new(|| {
    regex::bytes::Regex::new(
        r"(?i)\b(CREATE|INSERT|DROP|ALTER|SELECT|COPY|SET\s|BEGIN|COMMIT|--)\b",
    )
    .expect("padrao SQL de importacao invalido")
});

fn verify_sql(header: &[u8]) -> IntegrityResult {
    if SQL_PATTERN.is_match(header) {
        IntegrityResult::valid("Arquivo SQL válido")
    } else {
        IntegrityResult::invalid(
            "O arquivo não contém instruções SQL reconhecíveis no início \
             (esperado: CREATE, INSERT, COPY, etc.)",
        )
    }
}

fn verify_gzip(header: &[u8]) -> IntegrityResult {
    if header.starts_with(MAGIC_GZIP) {
        IntegrityResult::valid("Arquivo Gzip válido")
    } else {
        IntegrityResult::invalid(
            "Magic bytes inválidos — o arquivo não é um Gzip válido (esperado: 0x1F 0x8B)",
        )
    }
}

fn verify_pgdump(header: &[u8]) -> IntegrityResult {
    if header.starts_with(MAGIC_PGDUMP) {
        IntegrityResult::valid("Dump PostgreSQL no formato customizado (pg_dump -Fc) válido")
    } else {
        IntegrityResult::invalid(
            "Magic bytes inválidos — o arquivo não é um dump PostgreSQL no formato \
             customizado (esperado: PGDMP)",
        )
    }
}

fn verify_zip(header: &[u8]) -> IntegrityResult {
    if header.starts_with(MAGIC_ZIP) {
        IntegrityResult::valid_with_warning("Arquivo ZIP válido", NEEDS_EXTRACTION)
    } else {
        IntegrityResult::invalid(
            "Magic bytes inválidos — o arquivo não é um ZIP válido (esperado: PK\\x03\\x04)",
        )
    }
}

fn verify_tar(header: &[u8]) -> IntegrityResult {
    // Um `.tar.gz` e' gzip por fora: o `ustar` so' aparece depois de
    // descomprimir, e exigi-lo aqui reprovaria todo `.tgz` valido.
    if header.starts_with(MAGIC_GZIP) {
        return IntegrityResult::valid_with_warning(
            "Arquivo TAR.GZ (Gzip) válido",
            NEEDS_EXTRACTION,
        );
    }

    let ustar = header
        .get(TAR_MAGIC_OFFSET..TAR_MAGIC_OFFSET + 5)
        .unwrap_or_default();

    if ustar.starts_with(b"ustar") {
        IntegrityResult::valid_with_warning("Arquivo TAR válido", NEEDS_EXTRACTION)
    } else {
        IntegrityResult::invalid(
            "O arquivo não é um TAR válido (magic \"ustar\" não encontrado no offset 257)",
        )
    }
}

/// Quantos bytes bastam para todas as verificacoes.
///
/// 8 KB e' o que o `verifySqlFile` do Adonis le'; o `ustar` do TAR fica no
/// offset 257, bem dentro dessa janela.
pub const HEADER_BYTES: usize = 8192;

/// Nome de arquivo seguro para entrar num caminho.
///
/// Lista de permissao. Tudo o que nao for letra, digito, `.`, `_` ou `-` vira
/// `_`; `..` some. Um nome que sobre vazio recebe um default — devolver string
/// vazia produziria um caminho terminado em `/`.
#[must_use]
pub fn sanitize_file_name(original: &str) -> String {
    let cleaned: String = original
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect();

    // Depois da troca ainda pode restar `..`, que e' o unico componente capaz de
    // subir um nivel.
    let cleaned = cleaned.replace("..", "_");
    let cleaned = cleaned.trim_matches(['.', '_', '-']).to_string();

    if cleaned.is_empty() {
        "backup".to_string()
    } else {
        // O `file_path` inteiro cabe numa coluna de texto, mas um nome de 4 KB
        // estoura o limite do sistema de arquivos.
        cleaned.chars().take(180).collect()
    }
}

/// Nome do banco inferido do nome do arquivo, como no `inferDatabaseName`.
///
/// Serve de default quando o formulario nao traz `databaseName`: um backup sem
/// nome de banco aparece na listagem como "N/A" e nao da' para restaurar sem
/// digitar o destino a mao.
#[must_use]
pub fn infer_database_name(file_name: &str) -> String {
    static SUFFIX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)\.(sql\.gz|tar\.gz|tgz|sql|gz|dump|pgdump|pg_dump|zip|tar)$")
            .expect("padrao de extensao invalido")
    });
    static TIMESTAMP: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"[_-]?\d{8,}[_-]?").expect("padrao invalido")
    });
    static ROLE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)[_-]+(backup|dump|import)$").expect("padrao invalido")
    });

    let without_extension = SUFFIX.replace(file_name, "");
    let without_timestamp = TIMESTAMP.replace_all(&without_extension, "");
    let without_role = ROLE.replace(&without_timestamp, "");

    let normalized: String = without_role
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();

    let trimmed = normalized.trim_matches('_');

    if trimmed.is_empty() {
        "imported".to_string()
    } else {
        trimmed.chars().take(64).collect()
    }
}

/// Caminho relativo de destino de um arquivo importado.
///
/// A pasta e' o id da conexao, ou `imported` quando o backup nao esta' vinculado
/// a nenhuma — e' o mesmo layout do dump, e mantem os dois lado a lado na hora
/// de inspecionar o disco.
#[must_use]
pub fn build_relative_path(
    connection_id: Option<i64>,
    original_name: &str,
    epoch_millis: i64,
) -> String {
    let folder = connection_id.map_or_else(|| "imported".to_string(), |id| id.to_string());
    let safe = sanitize_file_name(original_name);

    format!("{folder}/import_{epoch_millis}_{safe}")
}

/// SHA-256 de um arquivo ja' gravado, lido em blocos.
///
/// Em blocos, e nao com um `read_to_end`: um `.sql` de 400 GB importado
/// derrubaria o processo antes de chegar ao `digest`.
pub async fn checksum_of(path: &Path) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_the_double_extensions_before_the_simple_ones() {
        // `extname()` devolveria `.gz` para os dois, e um `.tar.gz` seria
        // tratado como SQL comprimido.
        assert_eq!(resolve_extension("vendas.sql.gz"), ".sql.gz");
        assert_eq!(resolve_extension("vendas.tar.gz"), ".tar.gz");
        assert_eq!(resolve_extension("vendas.sql"), ".sql");
        assert_eq!(resolve_extension("VENDAS.SQL.GZ"), ".sql.gz");
        assert_eq!(resolve_extension("sem_extensao"), "");
    }

    #[test]
    fn maps_every_accepted_extension_to_a_format() {
        for (extension, expected) in [
            (".sql", ImportedFormat::Sql),
            (".sql.gz", ImportedFormat::SqlGz),
            (".gz", ImportedFormat::SqlGz),
            (".dump", ImportedFormat::Dump),
            (".pgdump", ImportedFormat::Dump),
            (".zip", ImportedFormat::Zip),
            (".tar", ImportedFormat::Tar),
            (".tar.gz", ImportedFormat::Tar),
            (".tgz", ImportedFormat::Tar),
        ] {
            assert_eq!(
                format_for_extension(extension),
                Some(expected),
                "{extension}"
            );
        }
    }

    #[test]
    fn refuses_an_executable_before_touching_the_disk() {
        // Aceitar `.exe` significaria gravar um executavel na area de backups.
        for name in ["malicioso.exe", "script.sh", "foto.png", "sem_extensao"] {
            assert!(
                matches!(
                    detect_format(name, b""),
                    Err(ImportError::UnsupportedExtension)
                ),
                "aceitou {name}"
            );
        }
    }

    #[test]
    fn a_bare_gz_is_confirmed_by_its_magic_bytes() {
        assert_eq!(
            detect_format("dados.gz", &[0x1f, 0x8b, 0x08]).ok(),
            Some(ImportedFormat::SqlGz)
        );
        assert!(matches!(
            detect_format("dados.gz", b"nao sou gzip"),
            Err(ImportError::NotGzip)
        ));
    }

    #[test]
    fn a_sql_gz_trusts_the_extension_pair() {
        // A extensao dupla e' explicita; so' a ambigua (`.gz`) exige o cabecalho.
        assert_eq!(
            detect_format("vendas.sql.gz", b"").ok(),
            Some(ImportedFormat::SqlGz)
        );
    }

    #[test]
    fn recognizes_a_real_sql_dump() {
        assert!(verify_integrity(ImportedFormat::Sql, b"-- MySQL dump\nCREATE TABLE t;").valid);
        assert!(!verify_integrity(ImportedFormat::Sql, b"apenas texto solto").valid);
    }

    #[test]
    fn recognizes_a_postgres_custom_dump() {
        assert!(verify_integrity(ImportedFormat::Dump, b"PGDMP\x01\x0e\x00").valid);
        assert!(!verify_integrity(ImportedFormat::Dump, b"PK\x03\x04").valid);
    }

    #[test]
    fn recognizes_a_zip_and_warns_that_it_needs_extraction() {
        let result = verify_integrity(ImportedFormat::Zip, &[0x50, 0x4b, 0x03, 0x04]);

        assert!(result.valid);
        assert!(result.warnings.is_some_and(|list| list.len() == 1));
    }

    #[test]
    fn accepts_a_gzipped_tar_without_looking_for_ustar() {
        // O `ustar` so' existe depois de descomprimir; exigi-lo reprovaria todo
        // `.tgz` valido.
        assert!(verify_integrity(ImportedFormat::Tar, &[0x1f, 0x8b, 0x08]).valid);
    }

    #[test]
    fn finds_the_ustar_magic_at_its_offset() {
        let mut header = vec![0_u8; HEADER_BYTES];
        header[TAR_MAGIC_OFFSET..TAR_MAGIC_OFFSET + 5].copy_from_slice(b"ustar");

        assert!(verify_integrity(ImportedFormat::Tar, &header).valid);
        assert!(!verify_integrity(ImportedFormat::Tar, &[0_u8; 300]).valid);
    }

    #[test]
    fn a_short_header_is_invalid_not_a_panic() {
        // O upload pode ser menor que o offset do `ustar`.
        assert!(!verify_integrity(ImportedFormat::Tar, b"abc").valid);
    }

    #[test]
    fn sanitizes_a_name_that_would_escape_the_backup_area() {
        for hostile in [
            "../../etc/passwd",
            "..\\..\\windows\\system32",
            "/etc/shadow",
            "a/b/c.sql",
        ] {
            let safe = sanitize_file_name(hostile);
            assert!(!safe.contains('/'), "{hostile} -> {safe}");
            assert!(!safe.contains('\\'), "{hostile} -> {safe}");
            assert!(!safe.contains(".."), "{hostile} -> {safe}");
        }
    }

    #[test]
    fn keeps_a_reasonable_name_recognizable() {
        assert_eq!(
            sanitize_file_name("vendas_20260809.sql.gz"),
            "vendas_20260809.sql.gz"
        );
    }

    #[test]
    fn never_produces_an_empty_name() {
        // Vazio produziria um caminho terminado em `/`.
        assert_eq!(sanitize_file_name(""), "backup");
        assert_eq!(sanitize_file_name("///"), "backup");
        assert_eq!(sanitize_file_name("..."), "backup");
    }

    #[test]
    fn caps_the_name_length() {
        assert!(sanitize_file_name(&"a".repeat(500)).len() <= 180);
    }

    #[test]
    fn infers_the_database_name_from_the_file_name() {
        assert_eq!(infer_database_name("estoque.sql"), "estoque");
        assert_eq!(infer_database_name("loja-backup.sql"), "loja");
        assert_eq!(infer_database_name("meu banco.dump"), "meu_banco");
    }

    #[test]
    fn only_the_first_number_block_of_the_timestamp_is_removed() {
        // Quirk herdado: o padrao `[_-]?\d{8,}[_-]?` consome os separadores dos
        // **dois** lados do bloco de 8+ digitos, e a hora (6 digitos) fica
        // colada no nome. `vendas_20260809_120000.sql.gz` vira `vendas120000`,
        // nao `vendas`.
        //
        // O nome inferido e' so' o default de `databaseName` quando o formulario
        // nao o envia, e ele aparece na listagem. "Corrigir" mudaria o valor
        // gravado para todo backup importado sem nome — decisao de produto, nao
        // do porte.
        assert_eq!(
            infer_database_name("vendas_20260809_120000.sql.gz"),
            "vendas120000"
        );
    }

    #[test]
    fn falls_back_when_nothing_survives_the_inference() {
        assert_eq!(infer_database_name("20260809.sql"), "imported");
        assert_eq!(infer_database_name(".sql"), "imported");
    }

    #[test]
    fn builds_the_destination_path_under_the_connection_folder() {
        assert_eq!(
            build_relative_path(Some(12), "vendas.sql.gz", 1_775_000_000_000),
            "12/import_1775000000000_vendas.sql.gz"
        );
        // Sem conexao o arquivo ainda precisa de uma casa.
        assert_eq!(
            build_relative_path(None, "vendas.sql.gz", 1_775_000_000_000),
            "imported/import_1775000000000_vendas.sql.gz"
        );
    }

    #[test]
    fn the_destination_path_never_climbs_out() {
        let path = build_relative_path(Some(1), "../../etc/passwd", 1);

        assert!(!path.contains(".."), "caminho: {path}");
        assert_eq!(path.matches('/').count(), 1, "caminho: {path}");
    }

    #[tokio::test]
    async fn hashes_a_file_in_blocks() {
        let dir = tempfile::tempdir().expect("diretorio temporario");
        let path = dir.path().join("dump.sql");
        let payload = b"CREATE TABLE t (id int);\n".repeat(5000);
        tokio::fs::write(&path, &payload).await.expect("grava");

        assert_eq!(
            checksum_of(&path).await.expect("calcula o checksum"),
            hex::encode(Sha256::digest(&payload))
        );
    }

    #[test]
    fn only_the_sql_gz_is_unwrapped_by_the_restore() {
        // Um `.dump` do `pg_dump -Fc` tambem e' comprimido, mas por dentro:
        // passar um gunzip nele produziria lixo.
        assert!(ImportedFormat::SqlGz.is_gzip_wrapped());
        for format in [
            ImportedFormat::Sql,
            ImportedFormat::Dump,
            ImportedFormat::Zip,
            ImportedFormat::Tar,
        ] {
            assert!(!format.is_gzip_wrapped(), "{format:?}");
        }
    }
}
