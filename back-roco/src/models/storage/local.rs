//! Adapter do sistema de arquivos local (tarefa 8.2 do roadmap).
//!
//! Porte de `local_explorer_adapter.ts`.
//!
//! ## A barreira de path traversal é a razão de este arquivo existir
//!
//! Todo caminho que chega da API — o `path` do `browse`, a `key` do `DELETE` —
//! passa por [`LocalExplorer::resolve`], que **canonicaliza** e confere que o
//! resultado continua sob a base. Comparar textos antes de resolver não bastaria:
//! `a/../../etc` só se revela depois de normalizado, e num sistema com links
//! simbólicos nem isso basta — daí a canonicalização de verdade quando o alvo
//! existe.

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;

use super::{
    BucketObject, ListOptions, ListPage, ObjectMetadata, ObjectReader, StorageError,
    StorageExplorer,
};
use crate::models::storage::config::LocalConfig;

pub struct LocalExplorer {
    base: PathBuf,
}

impl LocalExplorer {
    /// Cria o adapter, caindo no `backup_storage_path` quando a config não
    /// define `basePath` — é o que `getBasePath` faz.
    pub fn new(config: &LocalConfig, default_base_path: &str) -> Result<Self, StorageError> {
        let configured = config
            .base_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(default_base_path);

        if configured.is_empty() {
            return Err(StorageError::InvalidConfig);
        }

        Ok(Self {
            base: PathBuf::from(configured),
        })
    }

    /// Resolve um caminho relativo dentro da base, recusando qualquer fuga.
    ///
    /// Duas barreiras, e as duas são necessárias:
    ///
    /// 1. **componente a componente** — um `..` ou um componente absoluto é
    ///    recusado antes de tocar o disco. Sozinha, essa barreira já cobre o
    ///    caminho que não existe (um `DELETE` de chave inventada);
    /// 2. **canonicalização** — quando o alvo existe, o caminho real precisa
    ///    continuar sob a base real. É o que pega o link simbólico que aponta
    ///    para fora, que a primeira barreira não enxerga.
    fn resolve(&self, relative: &str) -> Result<PathBuf, StorageError> {
        // Um caminho que já começa na raiz é recusado, e **não** reinterpretado
        // como relativo. As duas leituras são seguras — `<base>/etc/passwd`
        // continua sob a base —, mas quem pede `/etc/passwd` está pedindo o
        // arquivo do sistema; devolver outro conteúdo com status 200 esconderia
        // a tentativa em vez de registrá-la. O `resolve()` do Node faz o
        // mesmo: lá o componente absoluto vence a base e o guard reprova.
        if relative.starts_with('/') || relative.starts_with('\\') {
            return Err(StorageError::PathTraversal);
        }

        let mut resolved = self.base.clone();

        for segment in relative.split(['/', '\\']) {
            match segment {
                "" | "." => {}
                ".." => return Err(StorageError::PathTraversal),
                _ if Path::new(segment).is_absolute() => return Err(StorageError::PathTraversal),
                // `C:` no meio do caminho substituiria a raiz no Windows.
                _ if segment.contains(':') => return Err(StorageError::PathTraversal),
                _ => resolved.push(segment),
            }
        }

        // A canonicalização só é possível no que já existe. Um alvo inexistente
        // já passou pela barreira de componentes, que é suficiente para ele.
        if let (Ok(real), Ok(real_base)) = (resolved.canonicalize(), self.base.canonicalize()) {
            if !real.starts_with(&real_base) {
                return Err(StorageError::PathTraversal);
            }
        }

        Ok(resolved)
    }

    /// Caminho relativo à base, com `/`, que é o formato das chaves da API.
    fn key_of(&self, path: &Path) -> String {
        let relative = path.strip_prefix(&self.base).unwrap_or(path);

        relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

#[async_trait]
impl StorageExplorer for LocalExplorer {
    async fn list_objects(
        &self,
        path: &str,
        options: &ListOptions,
    ) -> Result<ListPage, StorageError> {
        let target = self.resolve(path)?;
        let limit = options.effective_limit();

        let Ok(mut entries) = tokio::fs::read_dir(&target).await else {
            // Diretório ausente devolve página vazia, e não erro: é o que o
            // Adonis faz, e a interface trata "pasta vazia" e "pasta que não
            // existe" da mesma forma.
            return Ok(ListPage::default());
        };

        let mut objects = Vec::new();

        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();

            if let Some(filter) = options.prefix.as_deref().map(str::trim) {
                if !filter.is_empty() && !name.starts_with(filter) {
                    continue;
                }
            }

            let Ok(metadata) = entry.metadata().await else {
                // Arquivo removido entre o `read_dir` e o `stat`: pular é
                // melhor que derrubar a listagem inteira.
                continue;
            };

            let key = self.key_of(&entry.path());
            let modified = metadata
                .modified()
                .ok()
                .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339());

            objects.push(if metadata.is_dir() {
                BucketObject::directory(key)
            } else {
                let mut file = BucketObject::file(
                    key,
                    i64::try_from(metadata.len()).unwrap_or(i64::MAX),
                    modified.clone(),
                );
                file.last_modified = modified;
                file
            });
        }

        // Ordem estável: `read_dir` devolve na ordem do sistema de arquivos, e
        // sem ordenar a mesma pasta pagina diferente a cada chamada. O corte
        // por cursor vem depois, pela mesma razão: ele compara chaves, e só
        // faz sentido sobre uma lista já ordenada.
        objects.sort_by(|a, b| a.key.cmp(&b.key));

        if let Some(cursor) = options.cursor.as_deref() {
            objects.retain(|object| object.key.as_str() > cursor);
        }

        let is_truncated = objects.len() > limit;
        objects.truncate(limit);

        let next_cursor = is_truncated
            .then(|| objects.last().map(|object| object.key.clone()))
            .flatten();

        Ok(ListPage {
            objects,
            next_cursor,
            is_truncated,
        })
    }

    async fn object_metadata(&self, key: &str) -> Result<ObjectMetadata, StorageError> {
        let path = self.resolve(key)?;

        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|_| StorageError::NotFound(format!("Arquivo \"{key}\"")))?;

        Ok(ObjectMetadata {
            key: key.to_string(),
            size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
            last_modified: metadata
                .modified()
                .ok()
                .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()),
            // O Adonis não deduz o mime pela extensão aqui, e inventar um valor
            // mudaria o corpo da resposta.
            content_type: None,
            etag: None,
        })
    }

    async fn delete_object(&self, key: &str, is_directory: bool) -> Result<(), StorageError> {
        let path = self.resolve(key)?;

        // Segunda barreira, depois de resolver: uma chave que se reduz à
        // própria base apagaria a árvore inteira de backups.
        if path == self.base {
            return Err(StorageError::RootDeletion);
        }

        if tokio::fs::metadata(&path).await.is_err() {
            let label = if is_directory {
                "Diretório"
            } else {
                "Arquivo"
            };
            return Err(StorageError::NotFound(format!("{label} \"{key}\"")));
        }

        if is_directory {
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(StorageError::backend)
        } else {
            tokio::fs::remove_file(&path)
                .await
                .map_err(StorageError::backend)
        }
    }

    async fn test_connection(&self) -> Result<(), StorageError> {
        // Ler o diretório, e não só verificar que ele existe: é a permissão de
        // leitura que o backup vai precisar, e um diretório sem permissão
        // passaria num teste de mera existência.
        let _entries = tokio::fs::read_dir(&self.base).await.map_err(|err| {
            StorageError::Backend(format!(
                "Diretório não acessível ({}): {err}",
                self.base.display()
            ))
        })?;

        Ok(())
    }

    async fn put_file(&self, key: &str, source: &Path) -> Result<(), StorageError> {
        let destination = self.resolve(key)?;

        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(StorageError::backend)?;
        }

        // `copy`, e não `rename`: origem e destino podem estar em dispositivos
        // diferentes (o `/tmp` e o volume de dados num container), e o
        // `rename` falha com `EXDEV` nesse caso.
        tokio::fs::copy(source, &destination)
            .await
            .map_err(StorageError::backend)?;

        Ok(())
    }

    async fn read_object(&self, key: &str) -> Result<ObjectReader, StorageError> {
        let path = self.resolve(key)?;

        let file = tokio::fs::File::open(&path)
            .await
            .map_err(|_| StorageError::NotFound(format!("Arquivo \"{key}\"")))?;

        Ok(Box::pin(file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn explorer_with_tree() -> (tempfile::TempDir, LocalExplorer) {
        let dir = tempfile::tempdir().expect("diretorio temporario");
        let base = dir.path();

        tokio::fs::create_dir_all(base.join("12"))
            .await
            .expect("cria 12");
        tokio::fs::write(base.join("12/vendas.sql.gz"), b"dump")
            .await
            .expect("cria arquivo");
        tokio::fs::write(base.join("raiz.txt"), b"x")
            .await
            .expect("cria arquivo na raiz");

        let explorer = LocalExplorer::new(
            &LocalConfig {
                base_path: Some(base.to_string_lossy().into_owned()),
            },
            "/nao-usado",
        )
        .expect("adapter local");

        (dir, explorer)
    }

    #[tokio::test]
    async fn lists_the_root_of_the_destination() {
        let (_dir, explorer) = explorer_with_tree().await;

        let page = explorer
            .list_objects("", &ListOptions::default())
            .await
            .expect("lista");

        let keys: Vec<&str> = page.objects.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, vec!["12", "raiz.txt"]);
        assert!(page.objects[0].is_directory);
        assert_eq!(page.objects[1].size, Some(1));
    }

    #[tokio::test]
    async fn lists_one_level_only() {
        let (_dir, explorer) = explorer_with_tree().await;

        let page = explorer
            .list_objects("12", &ListOptions::default())
            .await
            .expect("lista");

        assert_eq!(page.objects.len(), 1);
        assert_eq!(page.objects[0].key, "12/vendas.sql.gz");
        assert_eq!(page.objects[0].name, "vendas.sql.gz");
    }

    #[tokio::test]
    async fn a_missing_directory_is_an_empty_page_not_an_error() {
        // A interface trata "pasta vazia" e "pasta que nao existe" igual.
        let (_dir, explorer) = explorer_with_tree().await;

        let page = explorer
            .list_objects("nao-existe", &ListOptions::default())
            .await
            .expect("pagina vazia");

        assert!(page.objects.is_empty());
        assert!(!page.is_truncated);
    }

    #[tokio::test]
    async fn refuses_every_shape_of_path_traversal() {
        let (_dir, explorer) = explorer_with_tree().await;

        for escape in [
            "..",
            "../..",
            "12/../..",
            "12\\..\\..",
            "/etc/passwd",
            "C:/Windows",
        ] {
            let listed = explorer.list_objects(escape, &ListOptions::default()).await;
            assert!(
                matches!(listed, Err(StorageError::PathTraversal)),
                "aceitou listar {escape:?}"
            );

            let deleted = explorer.delete_object(escape, false).await;
            assert!(
                matches!(deleted, Err(StorageError::PathTraversal)),
                "aceitou apagar {escape:?}"
            );
        }
    }

    #[tokio::test]
    async fn refuses_to_delete_the_base_itself() {
        // Uma chave que se reduz a' propria base apagaria a arvore inteira.
        let (_dir, explorer) = explorer_with_tree().await;

        for root in [".", "./", "12/.."] {
            let outcome = explorer.delete_object(root, true).await;
            assert!(
                matches!(
                    outcome,
                    Err(StorageError::RootDeletion | StorageError::PathTraversal)
                ),
                "aceitou apagar {root:?}"
            );
        }
    }

    #[tokio::test]
    async fn deletes_a_file_and_then_reports_it_missing() {
        let (_dir, explorer) = explorer_with_tree().await;

        explorer
            .delete_object("12/vendas.sql.gz", false)
            .await
            .expect("remove");

        let again = explorer.delete_object("12/vendas.sql.gz", false).await;
        assert!(matches!(again, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn deletes_a_directory_recursively() {
        let (_dir, explorer) = explorer_with_tree().await;

        explorer.delete_object("12", true).await.expect("remove");

        let page = explorer
            .list_objects("", &ListOptions::default())
            .await
            .expect("lista");
        assert_eq!(page.objects.len(), 1);
    }

    #[tokio::test]
    async fn reads_the_metadata_of_a_file() {
        let (_dir, explorer) = explorer_with_tree().await;

        let metadata = explorer
            .object_metadata("12/vendas.sql.gz")
            .await
            .expect("metadados");

        assert_eq!(metadata.size, 4);
        assert!(metadata.last_modified.is_some());
    }

    #[tokio::test]
    async fn a_missing_file_has_no_metadata() {
        let (_dir, explorer) = explorer_with_tree().await;

        assert!(matches!(
            explorer.object_metadata("nao-existe").await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn writes_and_reads_an_object_back() {
        use tokio::io::AsyncReadExt;

        let (dir, explorer) = explorer_with_tree().await;
        let source = dir.path().join("origem.sql");
        tokio::fs::write(&source, b"CREATE TABLE t;")
            .await
            .expect("origem");

        explorer
            .put_file("enviados/copia.sql", &source)
            .await
            .expect("envia");

        let mut reader = explorer
            .read_object("enviados/copia.sql")
            .await
            .expect("le de volta");
        let mut content = Vec::new();
        reader.read_to_end(&mut content).await.expect("le tudo");

        assert_eq!(content, b"CREATE TABLE t;");
    }

    #[tokio::test]
    async fn paginates_with_the_cursor() {
        let dir = tempfile::tempdir().expect("diretorio temporario");
        for index in 0..5 {
            tokio::fs::write(dir.path().join(format!("arquivo-{index}.txt")), b"x")
                .await
                .expect("cria");
        }

        let explorer = LocalExplorer::new(
            &LocalConfig {
                base_path: Some(dir.path().to_string_lossy().into_owned()),
            },
            "/nao-usado",
        )
        .expect("adapter");

        let first = explorer
            .list_objects(
                "",
                &ListOptions {
                    limit: Some(2),
                    ..ListOptions::default()
                },
            )
            .await
            .expect("primeira pagina");

        assert_eq!(first.objects.len(), 2);
        assert!(first.is_truncated);

        let second = explorer
            .list_objects(
                "",
                &ListOptions {
                    limit: Some(2),
                    cursor: first.next_cursor.clone(),
                    ..ListOptions::default()
                },
            )
            .await
            .expect("segunda pagina");

        // Sem sobreposicao: a segunda pagina comeca depois da ultima chave da
        // primeira.
        assert_eq!(second.objects.len(), 2);
        assert!(second.objects[0].key > first.objects[1].key);
    }

    #[tokio::test]
    async fn filters_by_prefix_inside_the_directory() {
        let (_dir, explorer) = explorer_with_tree().await;

        let page = explorer
            .list_objects(
                "",
                &ListOptions {
                    prefix: Some("raiz".to_string()),
                    ..ListOptions::default()
                },
            )
            .await
            .expect("lista");

        assert_eq!(page.objects.len(), 1);
        assert_eq!(page.objects[0].key, "raiz.txt");
    }

    #[tokio::test]
    async fn testing_a_missing_directory_fails_with_the_path() {
        let explorer = LocalExplorer::new(
            &LocalConfig {
                base_path: Some("/diretorio/que/nao/existe".to_string()),
            },
            "/nao-usado",
        )
        .expect("adapter");

        let error = explorer.test_connection().await.expect_err("devia falhar");
        assert!(error.message().contains("nao/existe") || error.message().contains("existe"));
    }

    #[tokio::test]
    async fn falls_back_to_the_configured_backup_path() {
        let dir = tempfile::tempdir().expect("diretorio temporario");

        let explorer = LocalExplorer::new(
            &LocalConfig { base_path: None },
            &dir.path().to_string_lossy(),
        )
        .expect("adapter");

        explorer.test_connection().await.expect("le o diretorio");
    }

    #[test]
    fn a_blank_base_path_falls_back_instead_of_pointing_at_the_root() {
        // `basePath: "  "` viraria `PathBuf::from("")`, que resolve para o
        // diretorio corrente — e o `browse` listaria o codigo-fonte.
        let explorer = LocalExplorer::new(
            &LocalConfig {
                base_path: Some("   ".to_string()),
            },
            "/storage/backups",
        )
        .expect("adapter");

        assert_eq!(explorer.base, PathBuf::from("/storage/backups"));
    }
}
