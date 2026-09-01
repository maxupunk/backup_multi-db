use backend::app::App;
use loco_rs::cli;
use migration::Migrator;

#[tokio::main]
async fn main() -> loco_rs::Result<()> {
    // Se LOCO_CONFIG_FOLDER não foi definido explicitamente, resolve defensivamente
    // permitindo execução tanto no container (/app/config), na pasta da crate (config)
    // ou na raiz do repositório (backend/config).
    if std::env::var("LOCO_CONFIG_FOLDER").is_err() {
        let candidates = ["config", "backend/config", "/app/config"];
        for candidate in candidates {
            if std::path::Path::new(candidate).is_dir() {
                std::env::set_var("LOCO_CONFIG_FOLDER", candidate);
                break;
            }
        }
    }
    cli::main::<App, Migrator>().await
}
