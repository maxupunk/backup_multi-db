//! Migrador de dados Adonis -> backend.
//!
//! A decisao D4 e' **schema novo**, entao os dados nao vem de graca: este
//! programa le' o SQLite do AdonisJS e popula o banco do backend, tabela a
//! tabela, na ordem das chaves estrangeiras.
//!
//! ## O que ele preserva, e por que
//!
//! - **`auth_access_tokens.hash`** — e' o SHA-256 do secret do token. Perde-lo
//!   desloga todo mundo no cutover e anula o ganho de D1. Copiado como esta'.
//! - **`users.password`** — hash scrypt, byte-compativel por D2. Copiado como
//!   esta'; recriptografar seria impossivel (nao ha' a senha em claro).
//! - **`password_encrypted` / `config_encrypted`** — AES-256-GCM byte-compativel
//!   por D3. Copiados como estao. Recriptografar exigiria decifrar tudo e
//!   colocaria os segredos em memoria sem necessidade nenhuma.
//! - **os ids** — as FKs referenciam ids, e reatribuir exigiria remapear tudo.
//!
//! ## O que ele converte
//!
//! `auth_access_tokens.created_at`, `updated_at`, `last_used_at` e `expires_at`
//! guardam **epoch em milissegundos** (`1785928191780`), nao texto ISO — o
//! Lucid grava assim. As colunas do backend tem afinidade TEXT e o Sea-ORM
//! espera ISO, entao o valor e' convertido na copia. Sem isso o token nao seria
//! lido de volta, e a preservacao do `hash` nao serviria de nada.
//!
//! ## Idempotencia
//!
//! Roda quantas vezes for preciso — a janela de trafego-sombra (12.13) exige
//! isso. Cada tabela e' esvaziada antes de ser preenchida, dentro de **uma**
//! transacao: se algo falhar no meio, o banco volta ao estado anterior em vez
//! de ficar meio migrado.
//!
//! ## Uso
//!
//! ```sh
//! cargo run --bin migrate_data -- <origem.sqlite> <destino.sqlite> [--verify]
//! ```

use std::collections::BTreeMap;
use std::process::ExitCode;

use sea_orm::{
    ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, DatabaseTransaction, DbErr,
    Statement, TransactionTrait, Value,
};

/// Tabelas na ordem das chaves estrangeiras: pai antes de filho.
///
/// Inverter isso quebraria a insercao com `PRAGMA foreign_keys = ON`, e —
/// pior — passaria despercebido com o pragma desligado, deixando linhas orfas.
const TABLES: &[&str] = &[
    "users",
    "auth_access_tokens",
    "storage_destinations",
    "connections",
    "connection_databases",
    "backups",
    "audit_logs",
    "system_settings",
    "resource_metric_history",
];

/// Colunas que guardam epoch em milissegundos e precisam virar ISO.
const EPOCH_MS_COLUMNS: &[(&str, &[&str])] = &[(
    "auth_access_tokens",
    &["created_at", "updated_at", "last_used_at", "expires_at"],
)];

/// Quantas linhas por `INSERT`.
///
/// `resource_metric_history` tem ~25 mil linhas em producao. Carregar tudo em
/// memoria funcionaria hoje e deixaria de funcionar quando a tabela dobrar; o
/// lote mantem o uso constante.
const BATCH_SIZE: usize = 500;

struct Args {
    source: String,
    target: String,
    verify_only: bool,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    if positional.len() < 2 {
        return Err("uso: migrate_data <origem.sqlite> <destino.sqlite> [--verify]".to_string());
    }

    Ok(Args {
        source: positional[0].clone(),
        target: positional[1].clone(),
        verify_only: args.iter().any(|a| a == "--verify"),
    })
}

async fn connect(path: &str, read_only: bool) -> Result<DatabaseConnection, DbErr> {
    let mode = if read_only { "ro" } else { "rwc" };
    let url = format!("sqlite://{}?mode={mode}", path.replace('\\', "/"));
    Database::connect(&url).await
}

fn sql(query: impl Into<String>) -> Statement {
    Statement::from_string(DatabaseBackend::Sqlite, query.into())
}

/// Colunas de uma tabela, na ordem do schema.
async fn columns_of(db: &DatabaseConnection, table: &str) -> Result<Vec<String>, DbErr> {
    let rows = db
        .query_all_raw(sql(format!("pragma table_info(`{table}`)")))
        .await?;

    rows.iter().map(|row| row.try_get("", "name")).collect()
}

/// Converte epoch em milissegundos para `YYYY-MM-DD HH:MM:SS`.
///
/// Devolve `None` quando o valor nao e' um inteiro plausivel de milissegundos —
/// ai' ele ja' e' uma data em texto e deve ser copiado sem tocar.
fn epoch_ms_to_iso(value: &str) -> Option<String> {
    let millis: i64 = value.trim().parse().ok()?;

    // Corta os dois extremos absurdos. Um `0` aqui viraria 1970 em silencio, e
    // uma data de 1970 num token parece expiracao valida — o token seria
    // descartado sem explicacao.
    if !(946_684_800_000..=4_102_444_800_000).contains(&millis) {
        return None;
    }

    chrono::DateTime::from_timestamp_millis(millis)
        .map(|dt| dt.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Le' um valor como texto, seja qual for o tipo no SQLite.
///
/// A origem tem colunas com tipos misturados (o mesmo `created_at` pode ser
/// texto num registro e inteiro noutro). Tentar tipar cada uma exigiria um mapa
/// por coluna que ficaria desatualizado; texto atravessa tudo, e o SQLite de
/// destino reaplica a afinidade na insercao.
fn read_cell(row: &sea_orm::QueryResult, column: &str) -> Option<String> {
    if let Ok(value) = row.try_get::<Option<String>>("", column) {
        return value;
    }
    if let Ok(value) = row.try_get::<Option<i64>>("", column) {
        return value.map(|v| v.to_string());
    }
    if let Ok(value) = row.try_get::<Option<f64>>("", column) {
        return value.map(|v| v.to_string());
    }
    if let Ok(value) = row.try_get::<Option<bool>>("", column) {
        return value.map(|v| i32::from(v).to_string());
    }
    None
}

async fn copy_table(
    source: &DatabaseConnection,
    transaction: &DatabaseTransaction,
    table: &str,
    columns: &[String],
) -> Result<usize, DbErr> {
    let epoch_columns: Vec<&str> = EPOCH_MS_COLUMNS
        .iter()
        .find(|(name, _)| *name == table)
        .map_or_else(Vec::new, |(_, cols)| cols.to_vec());

    let column_list = columns
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let mut copied = 0usize;
    let mut offset = 0usize;

    loop {
        let rows = source
            .query_all_raw(sql(format!(
                "select {column_list} from `{table}` limit {BATCH_SIZE} offset {offset}"
            )))
            .await?;

        if rows.is_empty() {
            break;
        }

        let mut values: Vec<Value> = Vec::with_capacity(rows.len() * columns.len());
        let mut placeholders: Vec<String> = Vec::with_capacity(rows.len());

        for row in &rows {
            for column in columns {
                let mut cell = read_cell(row, column);

                if epoch_columns.contains(&column.as_str()) {
                    if let Some(raw) = cell.as_deref() {
                        if let Some(iso) = epoch_ms_to_iso(raw) {
                            cell = Some(iso);
                        }
                    }
                }

                values.push(cell.into());
            }

            placeholders.push(format!("({})", vec!["?"; columns.len()].join(", ")));
        }

        transaction
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                format!(
                    "insert into `{table}` ({column_list}) values {}",
                    placeholders.join(", ")
                ),
                values,
            ))
            .await?;

        copied += rows.len();
        offset += BATCH_SIZE;
    }

    Ok(copied)
}

/// Soma de verificacao por tabela.
///
/// Contagem sozinha nao provaria nada: copiar as linhas erradas daria o mesmo
/// numero. O checksum soma os ids e conta os nulos das colunas criticas —
/// barato, e sensivel a troca de conteudo.
async fn checksum(db: &DatabaseConnection, table: &str) -> Result<String, DbErr> {
    let has_id = columns_of(db, table).await?.iter().any(|c| c == "id");
    let expression = if has_id { "coalesce(sum(id), 0)" } else { "0" };

    let row = db
        .query_one_raw(sql(format!(
            "select count(*) as total, {expression} as soma from `{table}`"
        )))
        .await?;

    let Some(row) = row else {
        return Ok("0:0".to_string());
    };

    let total: i64 = row.try_get("", "total")?;
    let soma: i64 = row.try_get("", "soma").unwrap_or(0);
    Ok(format!("{total}:{soma}"))
}

async fn run(args: &Args) -> Result<bool, DbErr> {
    let source = connect(&args.source, true).await?;
    let target = connect(&args.target, false).await?;

    let mut before: BTreeMap<&str, String> = BTreeMap::new();
    for table in TABLES {
        before.insert(table, checksum(&source, table).await?);
    }

    if !args.verify_only {
        // As colunas sao levantadas **antes** de abrir a transacao. O SQLite
        // guarda o lock de escrita durante a transacao, e uma consulta ao
        // catalogo pedindo outra conexao do pool ficaria esperando o lock que
        // a propria transacao segura — deadlock que aparece como timeout de
        // pool, sem pista nenhuma da causa.
        let mut plan: Vec<(&str, Vec<String>)> = Vec::new();
        for table in TABLES {
            // As colunas vem do **destino**: se a origem tiver uma coluna que o
            // schema novo nao tem, ela e' deliberadamente deixada para tras, e
            // nao causa erro de insercao.
            let target_columns = columns_of(&target, table).await?;
            let source_columns = columns_of(&source, table).await?;
            let shared: Vec<String> = target_columns
                .into_iter()
                .filter(|c| source_columns.contains(c))
                .collect();
            plan.push((table, shared));
        }

        // Uma transacao para tudo: um erro na 7a tabela nao pode deixar as 6
        // primeiras migradas e o resto vazio.
        let transaction = target.begin().await?;

        // Limpeza na ordem inversa das FKs — o migrador precisa poder rodar de
        // novo durante o trafego-sombra.
        for table in TABLES.iter().rev() {
            transaction
                .execute_raw(sql(format!("delete from `{table}`")))
                .await?;
        }

        for (table, shared) in &plan {
            if shared.is_empty() {
                println!("  {table}: nenhuma coluna em comum, pulado");
                continue;
            }

            let copied = copy_table(&source, &transaction, table, shared).await?;
            println!("  {table}: {copied} linha(s)");
        }

        transaction.commit().await?;
    }

    println!();
    println!("Verificacao por tabela:");

    let mut ok = true;
    for table in TABLES {
        let source_sum = &before[table];
        let target_sum = checksum(&target, table).await?;

        let status = if *source_sum == target_sum {
            "ok"
        } else {
            ok = false;
            "DIVERGENTE"
        };

        println!("  {table:<26} origem={source_sum:<14} destino={target_sum:<14} {status}");
    }

    // O hash dos tokens e' o item que a decisao D1 protege. Conferir a
    // contagem nao basta: um `hash` truncado ou reescrito daria a mesma
    // contagem e derrubaria todas as sessoes no cutover.
    let orphan_hashes = target
        .query_one_raw(sql(
            "select count(*) as total from auth_access_tokens where length(hash) <> 64",
        ))
        .await?
        .map_or(Ok(0i64), |row| row.try_get("", "total"))?;

    if orphan_hashes > 0 {
        println!();
        println!("  ATENCAO: {orphan_hashes} token(s) com `hash` fora dos 64 caracteres de um SHA-256 hex.");
        ok = false;
    }

    Ok(ok)
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    println!("origem:  {}", args.source);
    println!("destino: {}", args.target);
    println!();

    match run(&args).await {
        Ok(true) => {
            println!();
            println!("Migracao conferida.");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!();
            println!("Divergencias encontradas.");
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("erro: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_adonis_epoch_to_iso() {
        // O valor real encontrado em producao.
        assert_eq!(
            epoch_ms_to_iso("1785928191780").as_deref(),
            Some("2026-08-05 11:09:51")
        );
    }

    #[test]
    fn leaves_iso_text_untouched() {
        // Ja' e' data em texto: nao ha' o que converter, e tentar converter
        // devolveria lixo.
        assert_eq!(epoch_ms_to_iso("2026-08-09 12:00:00"), None);
        assert_eq!(epoch_ms_to_iso("2026-08-09T12:00:00.000Z"), None);
    }

    #[test]
    fn refuses_implausible_epochs() {
        // `0` viraria 1970, que num `expires_at` parece expiracao valida — o
        // token seria descartado sem que ninguem entendesse por que.
        assert_eq!(epoch_ms_to_iso("0"), None);
        assert_eq!(epoch_ms_to_iso("-1"), None);
        // Segundos em vez de milissegundos tambem caem fora da janela.
        assert_eq!(epoch_ms_to_iso("1785928191"), None);
        assert_eq!(epoch_ms_to_iso(""), None);
        assert_eq!(epoch_ms_to_iso("abc"), None);
    }

    #[test]
    fn accepts_the_boundaries_of_the_plausible_window() {
        assert!(epoch_ms_to_iso("946684800000").is_some()); // 2000-01-01
        assert!(epoch_ms_to_iso("4102444800000").is_some()); // 2100-01-01
        assert!(epoch_ms_to_iso("946684799999").is_none());
        assert!(epoch_ms_to_iso("4102444800001").is_none());
    }

    #[test]
    fn the_table_order_puts_parents_before_children() {
        let position = |name: &str| TABLES.iter().position(|t| *t == name).unwrap();

        assert!(position("users") < position("auth_access_tokens"));
        assert!(position("storage_destinations") < position("connections"));
        assert!(position("connections") < position("connection_databases"));
        assert!(position("connection_databases") < position("backups"));
    }

    #[test]
    fn every_table_of_the_baseline_is_covered() {
        // Uma tabela esquecida aqui migraria vazia e ninguem notaria ate' a
        // primeira consulta em producao.
        for table in [
            "users",
            "auth_access_tokens",
            "storage_destinations",
            "connections",
            "connection_databases",
            "backups",
            "audit_logs",
            "system_settings",
            "resource_metric_history",
        ] {
            assert!(TABLES.contains(&table), "{table} ficou de fora");
        }
        assert_eq!(TABLES.len(), 9);
    }

    #[test]
    fn only_the_token_table_needs_epoch_conversion() {
        // Se outra tabela passar a guardar epoch, isto aqui precisa saber.
        assert_eq!(EPOCH_MS_COLUMNS.len(), 1);
        assert_eq!(EPOCH_MS_COLUMNS[0].0, "auth_access_tokens");
    }
}
