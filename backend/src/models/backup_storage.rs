//! Onde o arquivo de backup mora, no disco local.
//!
//! Porte da parte **local** de `app/services/storage_destination_service.ts`.
//! Os adaptadores remotos (S3, GCS, Azure, SFTP) sao a Fase 8; ate' la' um
//! destino remoto e' tratado como "sem copia local", que e' exatamente o que
//! `getLocalBasePath` faz quando a config nao e' do tipo `local`.
//!
//! ## O caminho gravado no banco e' relativo
//!
//! `backups.file_path` guarda `<connection_id>/<arquivo>.sql.gz`, sempre com
//! `/`. A base sai do destino (ou de `backup_storage_path`), e e' resolvida na
//! hora de ler ou apagar. Gravar o caminho absoluto amarraria o registro a' arvore
//! de diretorios da maquina que gerou o backup — mover o volume, ou trocar o
//! destino default, deixaria todo o historico apontando para o nada.

use std::path::{Path, PathBuf};

use loco_rs::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::models::_entities::{connections, storage_destinations};
use crate::models::encryption::EncryptionService;
use crate::models::storage_destinations::StorageType;

/// Destino que a implementacao anterior usaria para uma conexao.
///
/// Ordem: o destino vinculado a' conexao, se estiver **ativo**; senao o destino
/// marcado como default, tambem ativo; senao nenhum — e o backup cai em
/// `backup_storage_path`.
///
/// A checagem de `status` nao e' detalhe: um destino remoto que perdeu a
/// credencial fica marcado como inativo, e continuar mandando backups para ele
/// produziria uma fila de falhas em vez de um arquivo no disco local.
pub async fn resolve_destination_for_connection(
    db: &impl ConnectionTrait,
    connection: &connections::Model,
) -> Result<Option<storage_destinations::Model>> {
    if let Some(id) = connection.storage_destination_id {
        let linked = storage_destinations::Entity::find_by_id(id).one(db).await?;

        if let Some(destination) = linked {
            if destination.status == ACTIVE_STATUS {
                return Ok(Some(destination));
            }
        }
    }

    Ok(storage_destinations::Entity::find()
        .filter(storage_destinations::Column::IsDefault.eq(true))
        .filter(storage_destinations::Column::Status.eq(ACTIVE_STATUS))
        .order_by_asc(storage_destinations::Column::Id)
        .one(db)
        .await?)
}

/// Destino associado a um backup ja' gravado, sem o fallback do default.
///
/// Diferente da resolucao por conexao **de proposito**: o arquivo esta' onde
/// foi escrito. Cair no destino default aqui faria o download de um backup
/// antigo procurar o arquivo no bucket errado.
pub async fn resolve_destination_for_backup(
    db: &impl ConnectionTrait,
    storage_destination_id: Option<i64>,
) -> Result<Option<storage_destinations::Model>> {
    let Some(id) = storage_destination_id else {
        return Ok(None);
    };

    Ok(storage_destinations::Entity::find_by_id(id).one(db).await?)
}

const ACTIVE_STATUS: &str = "active";

/// Base local de um destino: o `basePath` dele quando e' do tipo `local`, ou o
/// `backup_storage_path` da configuracao.
pub fn local_base_path(
    destination: Option<&storage_destinations::Model>,
    encryption: &EncryptionService,
    fallback: &str,
) -> PathBuf {
    let Some(destination) = destination else {
        return PathBuf::from(fallback);
    };

    if !is_local(destination) {
        return PathBuf::from(fallback);
    }

    destination
        .decrypted_config(encryption)
        .ok()
        .as_ref()
        .and_then(|config| config.get("basePath").and_then(serde_json::Value::as_str))
        .filter(|value| !value.is_empty())
        .map_or_else(|| PathBuf::from(fallback), PathBuf::from)
}

/// Caminho absoluto de um `file_path` relativo.
///
/// Recusa caminhos que escapem da base. O `file_path` vem do banco e hoje e'
/// escrito so' por nos, mas o registro de um backup **importado** carrega o
/// nome de arquivo enviado pelo usuario: sem esta barreira, um `..` no nome
/// transformaria `GET /:id/download` e o `DELETE` em leitura e remocao de
/// arquivo arbitrario.
pub fn local_full_path(base: &Path, relative: &str) -> Option<PathBuf> {
    // Um caminho que ja' comeca na raiz nao e' relativo a base nenhuma. O teste
    // por componente abaixo nao pega este caso: `"/etc/x".split('/')` produz um
    // primeiro componente **vazio**, que seria descartado em silencio e o
    // resultado apontaria para `<base>/etc/x`.
    if relative.is_empty() || relative.starts_with('/') || relative.starts_with('\\') {
        return None;
    }

    let mut resolved = base.to_path_buf();

    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            // Um componente absoluto (`C:\`, `/etc`) substituiria a base
            // inteira no `push` do `PathBuf`.
            _ if Path::new(part).is_absolute() => return None,
            _ if part.contains(':') => return None,
            _ => resolved.push(part),
        }
    }

    (resolved != base).then_some(resolved)
}

/// Garante que o diretorio existe, criando a arvore inteira quando falta.
pub async fn ensure_directory(path: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(path).await
}

/// Apaga a copia local de um backup, se existir.
///
/// A ausencia do arquivo **nao** e' erro: o backup pode ter sido enviado para
/// um destino remoto e a copia local removida depois do upload. Fazer o
/// `DELETE` falhar nesse caso deixaria o registro orfao no banco para sempre.
pub async fn delete_local_file(path: &Path) -> std::io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// O destino guarda o arquivo fora do disco local?
///
/// Enquanto os adaptadores da Fase 8 nao existem, e' esta funcao que diz ao
/// controller que a remocao remota ficou pendente — em vez de apagar so' a
/// copia local e reportar sucesso, escondendo o objeto que continua no bucket.
pub fn is_remote(destination: Option<&storage_destinations::Model>) -> bool {
    destination.is_some_and(|row| !is_local(row))
}

/// Um `type` ilegivel conta como **remoto**, e nao como local.
///
/// E' a escolha conservadora: tratar lixo na coluna como local faria o
/// `DELETE` reportar sucesso depois de apagar so' uma copia que talvez nem
/// exista, deixando o objeto remoto para tras sem aviso.
fn is_local(destination: &storage_destinations::Model) -> bool {
    matches!(destination.storage_type(), Ok(StorageType::Local))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PathBuf {
        PathBuf::from("/storage/backups")
    }

    #[test]
    fn resolves_a_relative_path_under_the_base() {
        let resolved = local_full_path(&base(), "12/vendas_20260809_120000.sql.gz");

        assert_eq!(
            resolved,
            Some(base().join("12").join("vendas_20260809_120000.sql.gz"))
        );
    }

    #[test]
    fn accepts_backslashes_as_separators() {
        // O migrador traz caminhos gravados no Windows pela implementacao anterior.
        assert_eq!(
            local_full_path(&base(), "12\\vendas.sql.gz"),
            Some(base().join("12").join("vendas.sql.gz"))
        );
    }

    #[test]
    fn refuses_to_climb_out_of_the_base() {
        // O nome de um backup importado vem do usuario. Sem esta barreira,
        // `GET /:id/download` viraria leitura de arquivo arbitrario.
        for escape in [
            "../../etc/passwd",
            "12/../../etc/passwd",
            "..",
            "12/..\\..\\segredo",
        ] {
            assert_eq!(local_full_path(&base(), escape), None, "aceitou {escape:?}");
        }
    }

    #[test]
    fn refuses_an_absolute_component() {
        // `PathBuf::push` com um componente absoluto descarta a base inteira.
        assert_eq!(local_full_path(&base(), "/etc/passwd"), None);
        assert_eq!(local_full_path(&base(), "C:/Windows/system.ini"), None);
        assert_eq!(local_full_path(&base(), "12/C:/x"), None);
    }

    #[test]
    fn refuses_an_empty_or_meaningless_path() {
        assert_eq!(local_full_path(&base(), ""), None);
        assert_eq!(local_full_path(&base(), "."), None);
        assert_eq!(local_full_path(&base(), "./"), None);
    }

    #[test]
    fn falls_back_to_the_configured_path_without_a_destination() {
        let encryption =
            EncryptionService::from_hex_key(&"ab".repeat(32)).expect("chave de teste valida");

        assert_eq!(
            local_base_path(None, &encryption, "/storage/backups"),
            base()
        );
    }
}
