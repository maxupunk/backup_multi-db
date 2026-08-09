//! Acesso aos bancos **gerenciados** — MySQL, MariaDB e PostgreSQL (tarefa 6.3).
//!
//! Nao confundir com o banco de controle da aplicacao, que e' SQLite e chega
//! pelo `AppContext`. Aqui as credenciais vem de uma linha de `connections`,
//! sao descriptografadas na hora e a conexao e' descartada no fim da operacao —
//! nunca ha' pool, porque cada requisicao pode falar com um servidor diferente.
//!
//! ## Por que fica em `models/`
//!
//! Nao e' um model de entidade, mas e' logica de dominio consumida por
//! `models::connections` e reusada pelo pipeline de backup da Fase 7. As
//! alternativas seriam uma pasta nova na raiz de `src/` — que o `AGENTS.md`
//! proibe — ou uma camada de "service" generica, que ele proibe explicitamente.
//! Mesmo criterio ja' usado em `models/system_monitor.rs`.
//!
//! ## Um crate novo: `sqlx`
//!
//! O Sea-ORM fala com o banco de controle; para abrir conexao arbitraria contra
//! um servidor de terceiro com timeout e TLS configuraveis, o caminho e' o
//! `sqlx` — que ja' esta' na arvore de dependencias como base do proprio
//! Sea-ORM. Aqui ele so' ganha as features `mysql` e `postgres`.

use std::time::{Duration, Instant};

use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::{AssertSqlSafe, Connection as SqlxConnection, MySqlConnection, PgConnection, Row};

use crate::models::connections::DatabaseType;

/// Teto de espera para abrir a conexao.
///
/// Os 10 s vem do `connectionTimeoutMillis`/`connectTimeout` do Adonis. Sem
/// teto, um host que engole pacotes em vez de recusar deixaria a requisicao
/// pendurada ate' o cliente desistir — e o worker do servidor preso junto.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Bancos de sistema que o MySQL nao deve listar, iguais aos do Adonis.
const MYSQL_SYSTEM_DATABASES: [&str; 4] =
    ["information_schema", "mysql", "performance_schema", "sys"];

/// Para onde conectar, com a senha ja' em claro.
///
/// A struct nao deriva `Debug` de proposito: um `{:?}` num log de erro
/// imprimiria a senha do banco do cliente.
pub struct DatabaseTarget {
    pub kind: DatabaseType,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    /// Banco ao qual se conectar. `None` usa o banco default do SGBD.
    pub database: Option<String>,
    pub ssl: bool,
}

impl DatabaseTarget {
    /// Banco a usar quando nenhum foi informado.
    ///
    /// Todo servidor precisa de um banco para autenticar; o Adonis cai em
    /// `postgres` ou `mysql`, que existem em qualquer instalacao padrao.
    fn effective_database(&self) -> &str {
        self.database.as_deref().unwrap_or(match self.kind {
            DatabaseType::Postgresql => "postgres",
            DatabaseType::Mysql | DatabaseType::Mariadb => "mysql",
        })
    }

    fn is_postgres(&self) -> bool {
        matches!(self.kind, DatabaseType::Postgresql)
    }
}

/// Resultado de um teste de conexao bem-sucedido.
#[derive(Debug, Clone)]
pub struct Probe {
    pub latency_ms: i64,
    pub version: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("{0}")]
    Connection(String),
    #[error("{0}")]
    Query(String),
    #[error("nome de banco de dados inválido: {0}")]
    InvalidIdentifier(String),
}

impl DriverError {
    /// Texto que vai para o campo `error` da resposta.
    ///
    /// E' a mensagem do SGBD, como no Adonis — e' ela que diz "senha errada" ou
    /// "host recusou a conexao", e escondê-la atras de um texto generico
    /// transformaria o botao "Testar" num oraculo inutil.
    pub fn message(&self) -> String {
        self.to_string()
    }
}

fn connection_error(err: &sqlx::Error) -> DriverError {
    DriverError::Connection(err.to_string())
}

fn query_error(err: &sqlx::Error) -> DriverError {
    DriverError::Query(err.to_string())
}

/// Mede latencia e le' a versao do servidor.
pub async fn probe(target: &DatabaseTarget) -> Result<Probe, DriverError> {
    let started = Instant::now();

    let version = if target.is_postgres() {
        let mut connection = connect_postgres(target).await?;
        let row = sqlx::query("SELECT version() as version")
            .fetch_one(&mut connection)
            .await
            .map_err(|err| query_error(&err))?;
        let version: Option<String> = row.try_get("version").ok();
        let _ = connection.close().await;
        version
    } else {
        let mut connection = connect_mysql(target).await?;
        let row = sqlx::query("SELECT VERSION() as version")
            .fetch_one(&mut connection)
            .await
            .map_err(|err| query_error(&err))?;
        let version: Option<String> = row.try_get("version").ok();
        let _ = connection.close().await;
        version
    };

    Ok(Probe {
        // A latencia inclui abrir a conexao, que e' o que o usuario percebe ao
        // clicar em "Testar" — medir so' a consulta daria sempre um numero
        // otimista de um digito.
        latency_ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
        version,
    })
}

/// Lista os bancos do servidor, sem os de sistema.
pub async fn list_databases(target: &DatabaseTarget) -> Result<Vec<String>, DriverError> {
    if target.is_postgres() {
        let mut connection = connect_postgres(target).await?;
        // Os mesmos filtros do Adonis: sem templates, sem bancos que nao
        // aceitam conexao, e sem o proprio `postgres`.
        let rows = sqlx::query(
            "SELECT datname FROM pg_database \
             WHERE datistemplate = false AND datallowconn = true \
             AND datname NOT IN ('postgres') ORDER BY datname",
        )
        .fetch_all(&mut connection)
        .await
        .map_err(|err| query_error(&err))?;
        let _ = connection.close().await;

        Ok(rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("datname").ok())
            .collect())
    } else {
        let mut connection = connect_mysql(target).await?;
        let rows = sqlx::query("SHOW DATABASES")
            .fetch_all(&mut connection)
            .await
            .map_err(|err| query_error(&err))?;
        let _ = connection.close().await;

        Ok(rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>(0).ok())
            .filter(|name| !MYSQL_SYSTEM_DATABASES.contains(&name.as_str()))
            .collect())
    }
}

/// Diz se `name` ja' existe no servidor.
pub async fn database_exists(target: &DatabaseTarget, name: &str) -> Result<bool, DriverError> {
    if target.is_postgres() {
        let mut connection = connect_postgres(target).await?;
        let row = sqlx::query("SELECT 1 AS found FROM pg_database WHERE datname = $1")
            .bind(name)
            .fetch_optional(&mut connection)
            .await
            .map_err(|err| query_error(&err))?;
        let _ = connection.close().await;
        Ok(row.is_some())
    } else {
        let mut connection = connect_mysql(target).await?;
        let row =
            sqlx::query("SELECT 1 AS found FROM information_schema.schemata WHERE schema_name = ?")
                .bind(name)
                .fetch_optional(&mut connection)
                .await
                .map_err(|err| query_error(&err))?;
        let _ = connection.close().await;
        Ok(row.is_some())
    }
}

/// Cria um banco de dados.
///
/// O nome **nao** pode ser parametrizado — nenhum SGBD aceita bind em DDL —, e
/// por isso ele passa por [`quote_identifier`] antes de entrar na string. A
/// validacao do controller ja' recusa qualquer coisa fora de
/// `[A-Za-z_][A-Za-z0-9_-]*`; a checagem aqui e' a segunda barreira, para o dia
/// em que outro chamador esquecer a primeira.
pub async fn create_database(target: &DatabaseTarget, name: &str) -> Result<(), DriverError> {
    let quoted = quote_identifier(name, target.is_postgres())?;

    if target.is_postgres() {
        let mut connection = connect_postgres(target).await?;
        // `AssertSqlSafe` porque DDL nao aceita bind: o nome ja' passou por
        // `quote_identifier`, que e' lista de permissao.
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {quoted}")))
            .execute(&mut connection)
            .await
            .map_err(|err| query_error(&err))?;
        let _ = connection.close().await;
    } else {
        let mut connection = connect_mysql(target).await?;
        // `utf8mb4` e' o mesmo charset que o `CREATE DATABASE` do Adonis usa.
        sqlx::query(AssertSqlSafe(format!(
            "CREATE DATABASE {quoted} CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci"
        )))
        .execute(&mut connection)
        .await
        .map_err(|err| query_error(&err))?;
        let _ = connection.close().await;
    }

    Ok(())
}

/// Aspas o identificador, recusando o que nao for claramente seguro.
///
/// Uma lista de permissao, e nao de bloqueio: escapar aspas seria suficiente na
/// teoria, mas DDL nao aceita parametro, e uma lista de bloqueio erra em
/// silencio quando aparece um caractere que ninguem previu.
fn quote_identifier(name: &str, postgres: bool) -> Result<String, DriverError> {
    let valid = !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    if !valid {
        return Err(DriverError::InvalidIdentifier(name.to_string()));
    }

    Ok(if postgres {
        format!("\"{name}\"")
    } else {
        format!("`{name}`")
    })
}

async fn connect_mysql(target: &DatabaseTarget) -> Result<MySqlConnection, DriverError> {
    let options = MySqlConnectOptions::new()
        .host(&target.host)
        .port(target.port)
        .username(&target.username)
        .password(&target.password)
        .database(target.effective_database())
        // Desligado salvo pedido explicito, igual ao `--skip-ssl` que o model
        // passa ao `mysqldump`: exigir TLS por default derrubaria as conexoes
        // com servidores que nao o suportam, que hoje funcionam.
        .ssl_mode(if target.ssl {
            MySqlSslMode::Required
        } else {
            MySqlSslMode::Disabled
        });

    with_timeout(MySqlConnection::connect_with(&options)).await
}

async fn connect_postgres(target: &DatabaseTarget) -> Result<PgConnection, DriverError> {
    let options = PgConnectOptions::new()
        .host(&target.host)
        .port(target.port)
        .username(&target.username)
        .password(&target.password)
        .database(target.effective_database())
        // `Prefer` negocia TLS quando o servidor oferece e cai para texto claro
        // quando nao — o mesmo conjunto de servidores que o `pg` do Adonis
        // aceita, com criptografia onde da'.
        .ssl_mode(if target.ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        });

    with_timeout(PgConnection::connect_with(&options)).await
}

async fn with_timeout<T>(
    future: impl std::future::Future<Output = Result<T, sqlx::Error>>,
) -> Result<T, DriverError> {
    match tokio::time::timeout(CONNECT_TIMEOUT, future).await {
        Ok(Ok(connection)) => Ok(connection),
        Ok(Err(err)) => Err(connection_error(&err)),
        Err(_) => Err(DriverError::Connection(format!(
            "tempo limite de {} s excedido ao conectar",
            CONNECT_TIMEOUT.as_secs()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: DatabaseType, database: Option<&str>) -> DatabaseTarget {
        DatabaseTarget {
            kind,
            host: "127.0.0.1".to_string(),
            port: 3306,
            username: "tester".to_string(),
            password: "secret".to_string(),
            database: database.map(ToString::to_string),
            ssl: false,
        }
    }

    #[test]
    fn falls_back_to_the_default_database_of_each_engine() {
        assert_eq!(
            target(DatabaseType::Postgresql, None).effective_database(),
            "postgres"
        );
        assert_eq!(
            target(DatabaseType::Mysql, None).effective_database(),
            "mysql"
        );
        assert_eq!(
            target(DatabaseType::Mariadb, None).effective_database(),
            "mysql"
        );
        assert_eq!(
            target(DatabaseType::Mysql, Some("app")).effective_database(),
            "app"
        );
    }

    #[test]
    fn quotes_with_the_syntax_of_each_engine() {
        assert_eq!(
            quote_identifier("app_fixture", false).unwrap(),
            "`app_fixture`"
        );
        assert_eq!(
            quote_identifier("app_fixture", true).unwrap(),
            "\"app_fixture\""
        );
    }

    #[test]
    fn refuses_anything_that_could_break_out_of_the_ddl() {
        // DDL nao aceita parametro; esta e' a barreira que resta.
        for hostile in [
            "app`; DROP DATABASE x; --",
            "app\"; DROP DATABASE x; --",
            "app fixture",
            "app;x",
            "1app",
            "",
            "app'",
            "app\\",
        ] {
            assert!(
                quote_identifier(hostile, false).is_err(),
                "aceitou {hostile:?}"
            );
            assert!(
                quote_identifier(hostile, true).is_err(),
                "aceitou {hostile:?}"
            );
        }
    }

    #[test]
    fn refuses_a_name_longer_than_the_engines_accept() {
        assert!(quote_identifier(&"a".repeat(63), false).is_ok());
        assert!(quote_identifier(&"a".repeat(64), false).is_err());
    }

    #[test]
    fn accepts_the_names_the_validator_allows() {
        for name in ["app", "_app", "app_2", "app-2", "App"] {
            assert!(quote_identifier(name, false).is_ok(), "recusou {name:?}");
        }
    }

    #[test]
    fn the_system_database_list_matches_the_adonis_one() {
        assert_eq!(
            MYSQL_SYSTEM_DATABASES,
            ["information_schema", "mysql", "performance_schema", "sys"]
        );
    }

    #[tokio::test]
    async fn a_closed_port_fails_fast_with_the_engine_message() {
        // A porta 1 nao tem servico; o erro precisa chegar ao usuario com o
        // motivo, e nao virar um texto generico.
        let mut unreachable = target(DatabaseType::Mysql, Some("app"));
        unreachable.port = 1;

        let error = probe(&unreachable).await.expect_err("devia falhar");
        assert!(!error.message().is_empty());
    }
}
