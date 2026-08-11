//! Entrada standalone das migrations.
//!
//! Existe para quebrar uma dependencia circular real: `cargo loco db migrate`
//! precisa que a aplicacao compile, a aplicacao precisa das entidades geradas,
//! e as entidades so' podem ser geradas de um banco ja' migrado. Com este
//! binario as migrations rodam sem a aplicacao:
//!
//! ```sh
//! DATABASE_URL="sqlite://backend_development.sqlite?mode=rwc" \
//!   cargo run -p migration -- up
//! ```
use sea_orm_migration::prelude::*;

#[tokio::main]
async fn main() {
    cli::run_cli(migration::Migrator).await;
}
