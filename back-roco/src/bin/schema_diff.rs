//! Comparador de schema (tarefa 4.8 do roadmap).
//!
//! Compara o SQLite gerado pelas migrations do Rust com o
//! `docs/schema-baseline.sql` — o `.schema` do banco de **producao** do
//! Adonis, gravado na Fase 0.
//!
//! ## Por que nao um diff de texto
//!
//! O Sea-ORM emite `"id" integer NOT NULL PRIMARY KEY AUTOINCREMENT` onde o
//! Knex emite `` `id` integer not null primary key autoincrement ``. Um `diff`
//! acusaria as 44 linhas como diferentes e nao diria nada. O que interessa e'
//! a **estrutura**: quais tabelas existem, quais colunas, de que tipo, se
//! aceitam nulo, e quais indices — inclusive se sao unicos.
//!
//! Por isso este programa normaliza os dois lados antes de comparar: tira
//! aspas e crases, baixa para minusculas, e agrupa por nome.
//!
//! ## Uso
//!
//! ```sh
//! cargo run --bin schema_diff -- back_roco_development.sqlite ../docs/schema-baseline.sql
//! ```
//!
//! Sai com codigo 1 quando ha' diferenca nao justificada — e' o que permite
//! usa-lo no CI.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

/// Tabelas que existem so' de um lado por motivo conhecido.
///
/// Cada entrada e' uma justificativa registrada, nao um silenciamento: o
/// programa **lista** o que ignorou, para que a lista nao cresca sem que
/// ninguem repare.
const IGNORED_TABLES: &[(&str, &str)] = &[
    (
        "adonis_schema",
        "controle de migrations do Adonis; o equivalente Rust e' `seaql_migrations`",
    ),
    ("adonis_schema_versions", "controle de migrations do Adonis"),
    (
        "seaql_migrations",
        "controle de migrations do Sea-ORM; o equivalente Adonis e' `adonis_schema`",
    ),
    (
        "sqlite_sequence",
        "tabela interna do SQLite, criada sozinha por AUTOINCREMENT",
    ),
];

#[derive(Debug, Default, PartialEq, Eq)]
struct Column {
    name: String,
    /// Tipo declarado, ja' normalizado.
    declared_type: String,
    not_null: bool,
}

#[derive(Debug, Default)]
struct Table {
    columns: BTreeMap<String, Column>,
}

#[derive(Debug)]
struct Index {
    table: String,
    columns: Vec<String>,
    unique: bool,
}

#[derive(Debug, Default)]
struct Schema {
    tables: BTreeMap<String, Table>,
    indexes: BTreeMap<String, Index>,
}

/// Tira crases, aspas e colchetes de um identificador SQL.
fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c| c == '`' || c == '"' || c == '[' || c == ']' || c == '\'')
        .to_lowercase()
}

/// Divide uma lista separada por virgulas respeitando parenteses.
///
/// Sem isso, `CHECK (x in ('a', 'b'))` seria cortado no meio e cada pedaco
/// viraria uma "coluna" inexistente.
fn split_top_level(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    let mut in_string = false;

    for ch in input.chars() {
        match ch {
            '\'' => {
                in_string = !in_string;
                current.push(ch);
            }
            '(' if !in_string => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_string => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 && !in_string => {
                parts.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Normaliza um tipo declarado para a **afinidade** do SQLite.
///
/// No SQLite o tipo e' apenas uma sugestao: o que vale sao as cinco
/// afinidades. `varchar(100)`, `varchar(255)` e `text` tem todas afinidade
/// TEXT e se comportam igual. Comparar o texto declarado acusaria diferencas
/// que nao existem na pratica — mas `datetime` (NUMERIC) contra
/// `datetime_text` (TEXT) e' uma diferenca **real**, e esta funcao a preserva.
fn type_affinity(declared: &str) -> &'static str {
    let lower = declared.to_lowercase();

    if lower.contains("int") {
        return "INTEGER";
    }
    if lower.contains("char") || lower.contains("clob") || lower.contains("text") {
        return "TEXT";
    }
    if lower.contains("blob") || lower.is_empty() {
        return "BLOB";
    }
    if lower.contains("real") || lower.contains("floa") || lower.contains("doub") {
        return "REAL";
    }
    "NUMERIC"
}

fn parse_create_table(statement: &str, schema: &mut Schema) {
    let Some(open) = statement.find('(') else {
        return;
    };
    let Some(close) = statement.rfind(')') else {
        return;
    };

    let header = &statement[..open];
    let name = header
        .split_whitespace()
        .last()
        .map(unquote)
        .unwrap_or_default();
    if name.is_empty() {
        return;
    }

    let mut table = Table::default();

    for part in split_top_level(&statement[open + 1..close]) {
        let lower = part.to_lowercase();
        // Constraints de tabela nao sao colunas.
        if lower.starts_with("foreign key")
            || lower.starts_with("primary key")
            || lower.starts_with("unique")
            || lower.starts_with("check")
            || lower.starts_with("constraint")
        {
            continue;
        }

        let mut tokens = part.split_whitespace();
        let Some(column_name) = tokens.next().map(unquote) else {
            continue;
        };

        // O tipo e' o que vem antes da primeira palavra-chave.
        let rest: Vec<&str> = tokens.collect();
        let declared: String = rest
            .iter()
            .take_while(|token| {
                let lower = token.to_lowercase();
                !matches!(
                    lower.as_str(),
                    "not" | "null" | "primary" | "default" | "check" | "unique" | "references"
                ) && !lower.starts_with("autoincrement")
            })
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");

        table.columns.insert(
            column_name.clone(),
            Column {
                name: column_name,
                declared_type: type_affinity(&declared).to_string(),
                not_null: lower.contains("not null"),
            },
        );
    }

    schema.tables.insert(name, table);
}

fn parse_create_index(statement: &str, schema: &mut Schema) {
    let lower = statement.to_lowercase();
    let unique = lower.contains("unique index");

    let Some(on_at) = lower.find(" on ") else {
        return;
    };
    let head = &statement[..on_at];
    let Some(name) = head.split_whitespace().last().map(unquote) else {
        return;
    };

    let tail = &statement[on_at + 4..];
    let Some(open) = tail.find('(') else {
        return;
    };
    let Some(close) = tail.rfind(')') else {
        return;
    };

    let table = unquote(&tail[..open]);
    let columns = split_top_level(&tail[open + 1..close])
        .iter()
        .map(|column| unquote(column.split_whitespace().next().unwrap_or(column)))
        .collect();

    schema.indexes.insert(
        name,
        Index {
            table,
            columns,
            unique,
        },
    );
}

fn parse_schema(sql: &str) -> Schema {
    let mut schema = Schema::default();

    // O `.schema` do SQLite separa por `;` no fim da instrucao. As instrucoes
    // podem ter varias linhas.
    for statement in sql.split(";\n").flat_map(|block| block.split(";\r\n")) {
        let trimmed = statement.trim().trim_end_matches(';').trim();
        let lower = trimmed.to_lowercase();

        if lower.starts_with("create table") {
            parse_create_table(trimmed, &mut schema);
        } else if lower.starts_with("create index") || lower.starts_with("create unique index") {
            parse_create_index(trimmed, &mut schema);
        }
    }

    schema
}

fn read_sqlite_schema(path: &str) -> Result<String, String> {
    let connection = rusqlite_lite::open(path)?;
    connection.schema_sql()
}

/// Leitor minimo de `sqlite_master`, sem depender de um crate de SQLite.
///
/// O binario roda com o mesmo `sea-orm` da aplicacao; abrir uma conexao
/// assincrona so' para ler um catalogo seria mais codigo, nao menos.
mod rusqlite_lite {
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};

    pub struct Connection {
        runtime: tokio::runtime::Runtime,
        db: sea_orm::DatabaseConnection,
    }

    pub fn open(path: &str) -> Result<Connection, String> {
        let runtime = tokio::runtime::Runtime::new().map_err(|err| err.to_string())?;
        let url = format!("sqlite://{}?mode=ro", path.replace('\\', "/"));
        let db = runtime
            .block_on(Database::connect(&url))
            .map_err(|err| format!("nao consegui abrir {path}: {err}"))?;

        Ok(Connection { runtime, db })
    }

    impl Connection {
        pub fn schema_sql(&self) -> Result<String, String> {
            let rows = self
                .runtime
                .block_on(self.db.query_all_raw(Statement::from_string(
                    DatabaseBackend::Sqlite,
                    "select sql from sqlite_master where sql is not null",
                )))
                .map_err(|err| err.to_string())?;

            let mut out = String::new();
            for row in rows {
                let sql: String = row.try_get("", "sql").map_err(|err| err.to_string())?;
                out.push_str(&sql);
                out.push_str(";\n");
            }
            Ok(out)
        }
    }
}

struct Report {
    differences: Vec<String>,
    ignored: Vec<String>,
    justified: Vec<String>,
}

/// Diferenca conhecida, aceita com motivo.
///
/// Ficam **listadas** na saida, e nao suprimidas: uma excecao que ninguem ve'
/// vira uma excecao que ninguem revisa.
fn justification(difference: &str) -> Option<&'static str> {
    if difference.contains("afinidade TEXT no Rust, NUMERIC no baseline") {
        return Some(
            "coluna `datetime`: o Sea-ORM emite `datetime_text` (afinidade TEXT) e o Knex              `datetime` (NUMERIC). Para data em ISO as duas se comportam igual. Declarar o tipo              cru igualaria o schema, mas o gerador de entidades passaria a mapear a coluna como              `String` em vez de `DateTime` — fidelidade cosmetica em troca de seguranca de tipo.              So' seria um problema se alguma coluna guardasse numero, e e' por isso que o              migrador converte os inteiros de `auth_access_tokens` para ISO",
        );
    }
    None
}

fn compare(generated: &Schema, baseline: &Schema) -> Report {
    let ignored_names: BTreeSet<&str> = IGNORED_TABLES.iter().map(|(name, _)| *name).collect();

    let mut differences = Vec::new();
    let mut ignored = Vec::new();

    for (name, reason) in IGNORED_TABLES {
        if generated.tables.contains_key(*name) || baseline.tables.contains_key(*name) {
            ignored.push(format!("tabela `{name}` ignorada: {reason}"));
        }
    }

    let generated_tables: BTreeSet<&String> = generated
        .tables
        .keys()
        .filter(|name| !ignored_names.contains(name.as_str()))
        .collect();
    let baseline_tables: BTreeSet<&String> = baseline
        .tables
        .keys()
        .filter(|name| !ignored_names.contains(name.as_str()))
        .collect();

    for missing in baseline_tables.difference(&generated_tables) {
        differences.push(format!(
            "tabela `{missing}` existe no baseline e falta no Rust"
        ));
    }
    for extra in generated_tables.difference(&baseline_tables) {
        differences.push(format!(
            "tabela `{extra}` existe no Rust e falta no baseline"
        ));
    }

    for name in generated_tables.intersection(&baseline_tables) {
        let left = &generated.tables[*name];
        let right = &baseline.tables[*name];

        let left_columns: BTreeSet<&String> = left.columns.keys().collect();
        let right_columns: BTreeSet<&String> = right.columns.keys().collect();

        for missing in right_columns.difference(&left_columns) {
            differences.push(format!(
                "`{name}.{missing}` existe no baseline e falta no Rust"
            ));
        }
        for extra in left_columns.difference(&right_columns) {
            differences.push(format!(
                "`{name}.{extra}` existe no Rust e falta no baseline"
            ));
        }

        for column in left_columns.intersection(&right_columns) {
            let a = &left.columns[*column];
            let b = &right.columns[*column];

            if a.declared_type != b.declared_type {
                differences.push(format!(
                    "`{name}.{column}`: afinidade {} no Rust, {} no baseline",
                    a.declared_type, b.declared_type
                ));
            }
            if a.not_null != b.not_null {
                differences.push(format!(
                    "`{name}.{column}`: NOT NULL={} no Rust, {} no baseline",
                    a.not_null, b.not_null
                ));
            }
        }
    }

    let generated_indexes: BTreeSet<&String> = generated.indexes.keys().collect();
    let baseline_indexes: BTreeSet<&String> = baseline.indexes.keys().collect();

    for missing in baseline_indexes.difference(&generated_indexes) {
        differences.push(format!(
            "indice `{missing}` existe no baseline e falta no Rust"
        ));
    }
    for extra in generated_indexes.difference(&baseline_indexes) {
        differences.push(format!(
            "indice `{extra}` existe no Rust e falta no baseline"
        ));
    }

    for name in generated_indexes.intersection(&baseline_indexes) {
        let a = &generated.indexes[*name];
        let b = &baseline.indexes[*name];

        if a.table != b.table {
            differences.push(format!(
                "indice `{name}`: tabela {} no Rust, {} no baseline",
                a.table, b.table
            ));
        }
        if a.columns != b.columns {
            differences.push(format!(
                "indice `{name}`: colunas {:?} no Rust, {:?} no baseline",
                a.columns, b.columns
            ));
        }
        if a.unique != b.unique {
            // Um unique que virou indice comum e' o pior tipo de diferenca:
            // nada quebra hoje, e um dia aparecem duas linhas duplicadas.
            differences.push(format!(
                "indice `{name}`: UNIQUE={} no Rust, {} no baseline",
                a.unique, b.unique
            ));
        }
    }

    let mut justified = Vec::new();
    differences.retain(|difference| match justification(difference) {
        Some(reason) => {
            justified.push(format!("{difference} — {reason}"));
            false
        }
        None => true,
    });

    Report {
        differences,
        ignored,
        justified,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let database = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "back_roco_development.sqlite".to_string());
    let baseline_path = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "../docs/schema-baseline.sql".to_string());

    let generated_sql = match read_sqlite_schema(&database) {
        Ok(sql) => sql,
        Err(err) => {
            eprintln!("erro: {err}");
            return ExitCode::FAILURE;
        }
    };

    let baseline_sql = match std::fs::read_to_string(&baseline_path) {
        Ok(sql) => sql,
        Err(err) => {
            eprintln!("erro ao ler {baseline_path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let generated = parse_schema(&generated_sql);
    let baseline = parse_schema(&baseline_sql);

    println!(
        "Rust:     {} tabelas, {} indices",
        generated.tables.len(),
        generated.indexes.len()
    );
    println!(
        "Baseline: {} tabelas, {} indices",
        baseline.tables.len(),
        baseline.indexes.len()
    );
    println!();

    let report = compare(&generated, &baseline);

    for note in &report.ignored {
        println!("  (ignorado) {note}");
    }
    if !report.ignored.is_empty() {
        println!();
    }

    if !report.justified.is_empty() {
        println!("{} diferenca(s) justificada(s):", report.justified.len());
        // Uma so' linha por motivo: 24 colunas com a mesma justificativa
        // encheriam a tela e esconderiam o resto.
        let mut seen = BTreeSet::new();
        for entry in &report.justified {
            let (column, reason) = entry.split_once(" — ").unwrap_or((entry, ""));
            if seen.insert(reason) {
                println!("  - {reason}");
            }
            println!("      {column}");
        }
        println!();
    }

    if report.differences.is_empty() {
        println!("Nenhuma diferenca estrutural.");
        return ExitCode::SUCCESS;
    }

    println!("{} diferenca(s):", report.differences.len());
    for difference in &report.differences {
        println!("  - {difference}");
    }

    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_declared_types_to_sqlite_affinity() {
        // `varchar(100)` e `text` se comportam igual no SQLite; acusar isso
        // como diferenca seria ruido.
        assert_eq!(type_affinity("varchar(100)"), "TEXT");
        assert_eq!(type_affinity("text"), "TEXT");
        assert_eq!(type_affinity("integer"), "INTEGER");
        assert_eq!(type_affinity("bigint"), "INTEGER");
        assert_eq!(type_affinity("float"), "REAL");
        // Mas `datetime` e `datetime_text` sao diferentes de verdade — e' o
        // que muda como o valor e' guardado.
        assert_eq!(type_affinity("datetime"), "NUMERIC");
        assert_eq!(type_affinity("datetime_text"), "TEXT");
    }

    #[test]
    fn splits_respecting_parentheses() {
        // Sem isso, o CHECK viraria varias "colunas" inexistentes.
        let parts = split_top_level("`a` text check (`a` in ('x', 'y')), `b` integer");
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("check"));
    }

    #[test]
    fn parses_a_knex_style_table() {
        let schema = parse_schema(
            "CREATE TABLE `users` (`id` integer not null primary key autoincrement, \
             `email` varchar(254) not null, `full_name` varchar(255) null);\n",
        );

        let table = &schema.tables["users"];
        assert_eq!(table.columns.len(), 3);
        assert!(table.columns["email"].not_null);
        assert!(!table.columns["full_name"].not_null);
    }

    #[test]
    fn parses_a_sea_orm_style_table_the_same_way() {
        // O ponto do normalizador: as duas sintaxes tem que dar a mesma
        // estrutura, senao o diff acusa 44 diferencas inuteis.
        let knex = parse_schema(
            "CREATE TABLE `users` (`id` integer not null primary key autoincrement, \
             `email` varchar(254) not null);\n",
        );
        let sea = parse_schema(
            "CREATE TABLE \"users\" ( \"id\" integer NOT NULL PRIMARY KEY AUTOINCREMENT, \
             \"email\" varchar(254) NOT NULL );\n",
        );

        assert_eq!(compare(&sea, &knex).differences.len(), 0);
    }

    #[test]
    fn detects_a_missing_column() {
        let a = parse_schema("CREATE TABLE `t` (`id` integer not null);\n");
        let b = parse_schema("CREATE TABLE `t` (`id` integer not null, `nome` text null);\n");

        let report = compare(&a, &b);
        assert_eq!(report.differences.len(), 1);
        assert!(report.differences[0].contains("nome"));
    }

    #[test]
    fn detects_a_unique_index_that_became_ordinary() {
        // A diferenca mais perigosa: nada quebra hoje, e um dia aparecem duas
        // linhas duplicadas.
        let a = parse_schema("CREATE INDEX `i` on `t` (`c`);\n");
        let b = parse_schema("CREATE UNIQUE INDEX `i` on `t` (`c`);\n");

        let report = compare(&a, &b);
        assert!(report.differences.iter().any(|d| d.contains("UNIQUE")));
    }

    #[test]
    fn detects_a_nullability_change() {
        let a = parse_schema("CREATE TABLE `t` (`c` text null);\n");
        let b = parse_schema("CREATE TABLE `t` (`c` text not null);\n");

        assert!(compare(&a, &b)
            .differences
            .iter()
            .any(|d| d.contains("NOT NULL")));
    }

    #[test]
    fn reports_ignored_tables_instead_of_hiding_them() {
        // Ignorar em silencio faria a lista de excecoes crescer sem ninguem
        // reparar.
        let a = parse_schema("CREATE TABLE `seaql_migrations` (`version` text);\n");
        let b = parse_schema("CREATE TABLE `adonis_schema` (`id` integer);\n");

        let report = compare(&a, &b);
        assert!(report.differences.is_empty());
        assert_eq!(report.ignored.len(), 2);
    }

    #[test]
    fn moves_the_datetime_affinity_to_the_justified_list() {
        // A diferenca continua existindo e continua sendo mostrada — so' nao
        // reprova o build.
        let rust = parse_schema(
            "CREATE TABLE `t` (`created_at` datetime_text not null);
",
        );
        let baseline = parse_schema(
            "CREATE TABLE `t` (`created_at` datetime not null);
",
        );

        let report = compare(&rust, &baseline);
        assert!(report.differences.is_empty(), "{:?}", report.differences);
        assert_eq!(report.justified.len(), 1);
        assert!(report.justified[0].contains("created_at"));
    }

    #[test]
    fn does_not_justify_a_real_type_change() {
        // A justificativa e' especifica. Um `integer` que virou `text` nao pode
        // pegar carona nela.
        let rust = parse_schema(
            "CREATE TABLE `t` (`c` text not null);
",
        );
        let baseline = parse_schema(
            "CREATE TABLE `t` (`c` integer not null);
",
        );

        assert_eq!(compare(&rust, &baseline).differences.len(), 1);
    }
}
