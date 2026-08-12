//! Espaço disponível por destino.
//!
//! Porte de `storage_space_service.ts`. Alimenta três lugares: as duas rotas de
//! espaço da interface legada e o bloco `storageSpaces` de `GET /api/stats`.
//!
//! ## Só o destino local tem espaço a informar
//!
//! S3, GCS, Azure e SFTP não expõem "quanto cabe ainda" — a cota, quando
//! existe, é da conta e não do bucket. a implementacao anterior devolve `null` para eles, e a
//! listagem agregada os inclui com `spaceAvailable: false` e zeros. Inventar um
//! total faria a interface desenhar uma barra de uso que não corresponde a nada.
//!
//! ## `statfs` não existe em Rust portável
//!
//! O Node lê `bsize`/`blocks`/`bfree` direto do `statfs`. Aqui a leitura vem do
//! `sysinfo`, que já está na árvore desde a Fase 5 — ver a nota sobre `bfree` em
//! [`filesystem_space`].

use std::path::{Path, PathBuf};

use loco_rs::prelude::ConnectionTrait;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::models::_entities::storage_destinations;
use crate::models::backup_storage;
use crate::models::encryption::EncryptionService;
use crate::models::storage_destinations::{Model, StorageType, DEFAULT_STATUS};

/// Abaixo disto o destino é considerado apertado.
pub const LOW_SPACE_THRESHOLD_PERCENT: f64 = 10.0;

/// Nome exibido para o disco local quando não há destino cadastrado.
pub const DEFAULT_LOCAL_NAME: &str = "Local (padrão)";

/// Espaço bruto de um sistema de arquivos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilesystemSpace {
    pub total: u64,
    pub free: u64,
}

/// O que uma rota de espaço devolve por destino.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceInfo {
    pub destination_id: Option<i64>,
    pub destination_name: String,
    pub storage_type: String,
    /// `false` nos destinos remotos, que não expõem uso.
    pub space_available: bool,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub used_percent: f64,
    pub free_percent: f64,
    pub is_low_space: bool,
    pub low_space_threshold: f64,
}

impl SpaceInfo {
    /// Linha de um destino que não sabe informar espaço.
    #[must_use]
    pub fn unavailable(destination: &Model) -> Self {
        Self {
            destination_id: Some(destination.id),
            destination_name: destination.name.clone(),
            storage_type: destination.r#type.clone(),
            space_available: false,
            total_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
            used_percent: 0.0,
            free_percent: 0.0,
            is_low_space: false,
            low_space_threshold: LOW_SPACE_THRESHOLD_PERCENT,
        }
    }
}

/// Espaço do sistema de arquivos que contém `path`.
///
/// `None` quando o caminho não existe ou não casa com nenhum ponto de montagem
/// — é o mesmo desfecho do `existsSync` que abre o serviço na implementacao anterior.
///
/// **Uma diferença registrada:** o `statfs` do Node usa `bfree`, que inclui os
/// blocos reservados ao root; o `sysinfo` expõe o equivalente a `bavail`, que
/// não os inclui. Num ext4 com os 5% de reserva padrão, o backend reporta
/// *menos* espaço livre que a implementacao anterior. A diferença empurra o alerta de espaço
/// baixo para **antes**, e não para depois — é o lado seguro de errar.
#[must_use]
pub fn filesystem_space(path: &Path) -> Option<FilesystemSpace> {
    if !path.exists() {
        return None;
    }

    // O caminho real: um link simbólico pode apontar para outra montagem, e o
    // espaço que interessa é o de onde o arquivo vai cair de fato.
    let canonical = path
        .canonicalize()
        .map(strip_verbatim)
        .unwrap_or_else(|_| path.to_path_buf());

    let disks = sysinfo::Disks::new_with_refreshed_list();

    // O caminho canônico primeiro; o original é a rede de segurança para o
    // ponto de montagem que só casa com a forma não resolvida.
    let disk = [canonical.as_path(), path]
        .into_iter()
        .find_map(|candidate| mount_of(&disks, candidate))?;

    Some(FilesystemSpace {
        total: disk.total_space(),
        free: disk.available_space(),
    })
}

/// O ponto de montagem mais específico que contém o caminho.
///
/// Sem o critério de comprimento, `/` casaria com tudo e o volume dedicado a
/// `/storage` nunca seria consultado.
fn mount_of<'a>(disks: &'a sysinfo::Disks, path: &Path) -> Option<&'a sysinfo::Disk> {
    disks
        .list()
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
}

/// Remove o prefixo `\\?\` que o `canonicalize` do Windows acrescenta.
///
/// Sem isto **nenhum** ponto de montagem casa no Windows: o `sysinfo` reporta
/// `C:\`, o `canonicalize` devolve `\\?\C:\…`, e o `starts_with` compara
/// componente a componente. O sintoma é `spaceAvailable` sumindo da resposta na
/// plataforma inteira — e um teste de unidade em `tempdir` já o revela.
///
/// Um UNC verbatim (`\\?\UNC\servidor\share`) **não** volta a ser um caminho
/// comum ao perder o prefixo, e por isso é devolvido intacto.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();

    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// Espaço de um destino, ou `None` quando ele não sabe informar.
///
/// `destination` a `None` significa o disco local padrão — é o que
/// `GET /api/stats` mostra numa instalação sem destino cadastrado.
#[must_use]
pub fn destination_space(
    destination: Option<&Model>,
    encryption: &EncryptionService,
    fallback: &str,
) -> Option<SpaceInfo> {
    // Um destino remoto não tem espaço a informar. A checagem vem antes da
    // leitura do disco: sem ela, o `basePath` cai no fallback e o serviço
    // reportaria o espaço da **máquina** como se fosse o do bucket.
    if let Some(destination) = destination {
        if !matches!(destination.storage_type(), Ok(StorageType::Local)) {
            return None;
        }
    }

    let base: PathBuf = backup_storage::local_base_path(destination, encryption, fallback);
    let space = filesystem_space(&base)?;

    let used_bytes = space.total.saturating_sub(space.free);
    let used_percent = if space.total > 0 {
        (used_bytes as f64 / space.total as f64) * 100.0
    } else {
        0.0
    };
    let free_percent = 100.0 - used_percent;

    Some(SpaceInfo {
        destination_id: destination.map(|row| row.id),
        destination_name: destination
            .map_or_else(|| DEFAULT_LOCAL_NAME.to_string(), |row| row.name.clone()),
        storage_type: destination.map_or_else(
            || StorageType::Local.as_str().to_string(),
            |row| row.r#type.clone(),
        ),
        space_available: true,
        total_bytes: space.total,
        used_bytes,
        free_bytes: space.free,
        used_percent: round_hundredths(used_percent),
        free_percent: round_hundredths(free_percent),
        is_low_space: free_percent < LOW_SPACE_THRESHOLD_PERCENT,
        low_space_threshold: LOW_SPACE_THRESHOLD_PERCENT,
    })
}

/// Espaço de todos os destinos ativos, mais o disco local padrão.
///
/// O disco padrão só entra quando **não** há um destino local marcado como
/// default: com ele cadastrado, as duas linhas apontariam para o mesmo volume e
/// a interface somaria o mesmo espaço duas vezes.
pub async fn all_destinations_space(
    db: &impl ConnectionTrait,
    encryption: &EncryptionService,
    fallback: &str,
) -> loco_rs::Result<Vec<SpaceInfo>> {
    let destinations = storage_destinations::Entity::find()
        .filter(storage_destinations::Column::Status.eq(DEFAULT_STATUS))
        .order_by_asc(storage_destinations::Column::Name)
        .all(db)
        .await?;

    let mut results = Vec::with_capacity(destinations.len() + 1);

    let has_default_local = destinations
        .iter()
        .any(|row| row.is_default && matches!(row.storage_type(), Ok(StorageType::Local)));

    if !has_default_local {
        if let Some(default) = destination_space(None, encryption, fallback) {
            results.push(default);
        }
    }

    for destination in &destinations {
        results.push(
            destination_space(Some(destination), encryption, fallback)
                .unwrap_or_else(|| SpaceInfo::unavailable(destination)),
        );
    }

    Ok(results)
}

/// Resultado da checagem que antecede um backup.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceCheck {
    pub has_enough_space: bool,
    pub free_percent: f64,
    pub free_bytes: u64,
    /// Texto do aviso, quando o destino está apertado.
    pub warning: Option<String>,
}

/// Confere o espaço antes de um backup.
///
/// Um destino sem informação (remoto, ou caminho que ainda não existe) é tratado
/// como **tendo** espaço: recusar o backup por não saber medir seria pior que
/// tentar e falhar com o erro real do disco.
#[must_use]
pub fn check_before_backup(
    destination: Option<&Model>,
    encryption: &EncryptionService,
    fallback: &str,
) -> SpaceCheck {
    let Some(info) = destination_space(destination, encryption, fallback) else {
        return SpaceCheck {
            has_enough_space: true,
            free_percent: 100.0,
            free_bytes: 0,
            warning: None,
        };
    };

    // `hasEnoughSpace` continua `true` mesmo no aviso: a implementacao anterior **não** bloqueia
    // o backup por espaço baixo, só avisa. Bloquear aqui mudaria o contrato.
    SpaceCheck {
        has_enough_space: true,
        free_percent: info.free_percent,
        free_bytes: info.free_bytes,
        warning: info.is_low_space.then(|| {
            format!(
                "Espaço em disco baixo no armazenamento \"{}\": apenas {:.1}% livre ({}). \
                 Recomenda-se ter pelo menos {}% de espaço livre.",
                info.destination_name,
                info.free_percent,
                format_bytes(info.free_bytes),
                LOW_SPACE_THRESHOLD_PERCENT as i64
            )
        }),
    }
}

/// Duas casas decimais, como o `Math.round(x * 100) / 100`.
fn round_hundredths(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Bytes em texto legível, com até duas casas e sem zeros à direita.
///
/// É o `formatBytes` do serviço, que usa `parseFloat(x.toFixed(2))` — `1.50 GB`
/// sai como `1.5 GB`, e `2.00 GB` como `2 GB`.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    // `{:.2}` e depois corta o zero à direita: é o que o `parseFloat` faz.
    let rendered = format!("{value:.2}");
    let trimmed = rendered.trim_end_matches('0').trim_end_matches('.');

    format!("{trimmed} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encryption() -> EncryptionService {
        EncryptionService::from_hex_key(&"a".repeat(64)).expect("chave de teste")
    }

    #[test]
    fn formats_bytes_the_way_the_service_does() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1 KB");
        // `parseFloat` corta o zero: `1.50 GB` vira `1.5 GB`.
        assert_eq!(
            format_bytes(1024 * 1024 * 1024 + 512 * 1024 * 1024),
            "1.5 GB"
        );
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2 GB");
    }

    #[test]
    fn rounds_percentages_to_two_decimals() {
        assert_eq!(round_hundredths(33.333_333), 33.33);
        assert_eq!(round_hundredths(66.666_666), 66.67);
        assert_eq!(round_hundredths(100.0), 100.0);
    }

    #[test]
    fn reads_the_space_of_a_real_directory() {
        let dir = tempfile::tempdir().expect("diretorio temporario");
        let space = filesystem_space(dir.path()).expect("o disco do temp existe");

        assert!(space.total > 0, "total zerado");
        assert!(space.free <= space.total);
    }

    #[test]
    fn strips_the_windows_verbatim_prefix() {
        // Sem isto nenhum ponto de montagem casa no Windows: o `sysinfo`
        // reporta `C:\` e o `canonicalize` devolve `\\?\C:\…`.
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\C:\storage\backups")),
            PathBuf::from(r"C:\storage\backups")
        );
        // UNC verbatim nao vira caminho comum ao perder o prefixo.
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\UNC\servidor\share")),
            PathBuf::from(r"\\?\UNC\servidor\share")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from("/storage/backups")),
            PathBuf::from("/storage/backups")
        );
    }

    #[test]
    fn a_missing_path_has_no_space() {
        let missing = std::env::temp_dir().join("backend-caminho-que-nao-existe");
        assert_eq!(filesystem_space(&missing), None);
    }

    #[test]
    fn the_default_disk_is_named_and_typed_as_local() {
        let dir = tempfile::tempdir().expect("diretorio temporario");
        let info = destination_space(None, &encryption(), &dir.path().to_string_lossy())
            .expect("o disco do temp existe");

        assert_eq!(info.destination_id, None);
        assert_eq!(info.destination_name, DEFAULT_LOCAL_NAME);
        assert_eq!(info.storage_type, "local");
        assert!(info.space_available);
        assert_eq!(info.low_space_threshold, LOW_SPACE_THRESHOLD_PERCENT);
        // Os dois lados da conta fecham em 100.
        assert!((info.used_percent + info.free_percent - 100.0).abs() < 0.02);
    }

    #[test]
    fn a_remote_destination_reports_nothing() {
        // Sem esta guarda o `basePath` cairia no fallback e o servico
        // reportaria o espaco da maquina como se fosse o do bucket.
        let mut destination = destination_row();
        destination.r#type = "s3".to_string();

        assert_eq!(
            destination_space(Some(&destination), &encryption(), "."),
            None
        );
    }

    #[test]
    fn an_unavailable_row_carries_zeros_and_the_threshold() {
        let mut destination = destination_row();
        destination.r#type = "s3".to_string();

        let info = SpaceInfo::unavailable(&destination);

        assert!(!info.space_available);
        assert_eq!(info.total_bytes, 0);
        assert_eq!(info.free_percent, 0.0);
        assert_eq!(info.low_space_threshold, LOW_SPACE_THRESHOLD_PERCENT);
        assert_eq!(info.destination_id, Some(destination.id));
    }

    #[test]
    fn a_destination_without_space_information_does_not_block_the_backup() {
        // Recusar o backup por nao saber medir seria pior que tentar e falhar
        // com o erro real do disco.
        let mut destination = destination_row();
        destination.r#type = "sftp".to_string();

        let check = check_before_backup(Some(&destination), &encryption(), ".");

        assert!(check.has_enough_space);
        assert_eq!(check.free_percent, 100.0);
        assert!(check.warning.is_none());
    }

    #[test]
    fn a_healthy_disk_produces_no_warning() {
        let dir = tempfile::tempdir().expect("diretorio temporario");
        let check = check_before_backup(None, &encryption(), &dir.path().to_string_lossy());

        assert!(check.has_enough_space);
        // O disco do CI pode estar apertado; o que se afirma aqui e' que o
        // aviso so' aparece junto com o percentual baixo.
        assert_eq!(
            check.warning.is_some(),
            check.free_percent < LOW_SPACE_THRESHOLD_PERCENT
        );
    }

    fn destination_row() -> Model {
        Model {
            id: 3,
            name: "Destino".to_string(),
            r#type: "local".to_string(),
            status: "active".to_string(),
            is_default: false,
            config_encrypted: String::new(),
            created_at: chrono::NaiveDateTime::parse_from_str(
                "2026-08-09 12:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .expect("data de teste")
            .and_utc()
            .fixed_offset(),
            updated_at: chrono::NaiveDateTime::parse_from_str(
                "2026-08-09 12:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .expect("data de teste")
            .and_utc()
            .fixed_offset(),
            provider: None,
        }
    }
}
