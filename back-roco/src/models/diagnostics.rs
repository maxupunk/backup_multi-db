//! Artefatos de diagnostico gravados em `DIAGNOSTICS_PATH` (tarefa 11.7).
//!
//! O Adonis gera heap snapshots e profiles do V8; em Rust nao ha' V8, mas o
//! diretorio e a API continuam os mesmos para nao quebrar o painel. Apenas
//! arquivos com as extensoes `.heapsnapshot`, `.cpuprofile` e `.heapprofile`
//! sao listados, baixados ou removidos.
//!
//! Acesso restrito a administradores: um heap snapshot e' material mais
//! sensivel que um backup, pois pode conter segredos em memoria. O controller
//! faz a checagem de `is_admin` e registra download/remocao em auditoria.

use std::path::{Path, PathBuf};

use loco_rs::prelude::*;
use serde::Serialize;

use crate::initializers::settings::Settings;

const ALLOWED_EXTENSIONS: &[&str] = &[".heapsnapshot", ".cpuprofile", ".heapprofile"];

/// Um artefato de diagnostico listado.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFile {
    pub name: String,
    pub size_bytes: u64,
    #[serde(serialize_with = "crate::views::timestamp::serialize_option")]
    pub created_at: Option<chrono::NaiveDateTime>,
    #[serde(serialize_with = "crate::views::timestamp::serialize_option")]
    pub modified_at: Option<chrono::NaiveDateTime>,
}

/// Resposta de `GET /api/system/diagnostics`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsOverview {
    pub directory: String,
    pub directory_exists: bool,
    pub files: Vec<DiagnosticFile>,
}

/// Lista os artefatos disponiveis, do mais recente para o mais antigo.
pub async fn list(ctx: &AppContext) -> Result<DiagnosticsOverview> {
    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let directory = PathBuf::from(&settings.diagnostics_path);
    let directory_exists = tokio::fs::metadata(&directory).await.is_ok();

    let mut files = Vec::new();

    if directory_exists {
        let mut entries = tokio::fs::read_dir(&directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if !metadata.is_file() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            if !has_allowed_extension(&name) {
                continue;
            }

            files.push(DiagnosticFile {
                size_bytes: metadata.len(),
                created_at: metadata.created().ok().and_then(into_naive),
                modified_at: metadata.modified().ok().and_then(into_naive),
                name,
            });
        }
    }

    files.sort_by_key(|b| std::cmp::Reverse(b.modified_at));

    Ok(DiagnosticsOverview {
        directory: settings.diagnostics_path,
        directory_exists,
        files,
    })
}

/// Resolve o caminho absoluto de um artefato a partir do nome.
///
/// Retorna `Ok(None)` para qualquer entrada suspeita: nome vazio, separador de
/// caminho, `..`, extensao fora da allowlist, caminho que escape do diretorio
/// ou arquivo inexistente.
pub fn resolve(ctx: &AppContext, file_name: &str) -> Result<Option<PathBuf>> {
    let trimmed = file_name.trim();

    if trimmed.is_empty() || trimmed != Path::new(trimmed).file_name().unwrap_or_default() {
        return Ok(None);
    }

    if trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') {
        return Ok(None);
    }

    if !has_allowed_extension(trimmed) {
        return Ok(None);
    }

    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let directory = PathBuf::from(&settings.diagnostics_path)
        .canonicalize()
        .map_err(|err| {
            Error::Message(format!(
                "diretorio de diagnosticos invalido ({}): {err}",
                settings.diagnostics_path
            ))
        })?;

    let target = directory.join(trimmed);
    if std::fs::metadata(&target).is_err() {
        return Ok(None);
    }

    if target != directory && !target.starts_with(&directory) {
        return Ok(None);
    }

    if !target.is_file() {
        return Ok(None);
    }

    Ok(Some(target))
}

/// Remove um artefato previamente validado.
pub async fn remove(path: &Path) -> Result<()> {
    tokio::fs::remove_file(path).await?;
    Ok(())
}

fn has_allowed_extension(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    ALLOWED_EXTENSIONS
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn into_naive(system_time: std::time::SystemTime) -> Option<chrono::NaiveDateTime> {
    let duration = system_time.duration_since(std::time::UNIX_EPOCH).ok()?;
    chrono::DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
        .map(|dt| dt.naive_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_ctx(diagnostics_path: &str) -> AppContext {
        let mut ctx = loco_rs::testing::prelude::boot_test::<crate::app::App>()
            .await
            .expect("test boot")
            .app_context;
        let mut settings = Settings::from_json(ctx.config.settings.as_ref()).unwrap();
        settings.diagnostics_path = diagnostics_path.into();
        ctx.config.settings = Some(serde_json::to_value(settings).unwrap());
        ctx
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn rejects_path_traversal_attempts() {
        let ctx = test_ctx("/storage/diagnostics").await;
        assert!(resolve(&ctx, "../etc/passwd.heapsnapshot")
            .unwrap()
            .is_none());
        assert!(resolve(&ctx, "subdir/file.heapsnapshot").unwrap().is_none());
        assert!(resolve(&ctx, "..\\windows\\file.heapsnapshot")
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn rejects_unknown_extensions() {
        let ctx = test_ctx("/storage/diagnostics").await;
        assert!(resolve(&ctx, "notes.txt").unwrap().is_none());
        assert!(resolve(&ctx, "script.sh").unwrap().is_none());
    }

    #[test]
    fn allowed_extensions_are_case_insensitive() {
        assert!(has_allowed_extension("dump.HEAPSNAPSHOT"));
        assert!(has_allowed_extension("profile.CpuProfile"));
        assert!(!has_allowed_extension("dump.txt"));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn resolves_existing_allowed_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("dump.heapsnapshot");
        tokio::fs::write(&file_path, b"heap").await.unwrap();

        let ctx = test_ctx(&dir.path().to_string_lossy()).await;
        let settings = Settings::from_json(ctx.config.settings.as_ref()).unwrap();
        assert_eq!(settings.diagnostics_path, dir.path().to_string_lossy());
        assert!(file_path.exists(), "arquivo de teste deve existir");

        let resolved = resolve(&ctx, "dump.heapsnapshot").unwrap();
        assert!(
            resolved.as_ref().map(|p| p.exists()).unwrap_or(false),
            "resolve deve devolver o caminho de um arquivo existente"
        );

        assert!(resolve(&ctx, "missing.heapsnapshot").unwrap().is_none());
    }
}
