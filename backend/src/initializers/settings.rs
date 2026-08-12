//! Configuracao especifica da aplicacao.
//!
//! O Loco tem um bloco `settings:` livre no YAML que chega como
//! `serde_json::Value`. Este modulo o transforma numa struct tipada, com uma
//! regra: **falhar no boot**, nunca em runtime.
//!
//! Uma `db_encryption_key` ausente tem que derrubar o processo no boot, e nao
//! na primeira requisicao que tenta descriptografar uma senha — que pode ser
//! semanas depois do deploy. E' a diferenca entre um erro de configuracao
//! evidente e uma falha intermitente.
//!
//! Por isso a validacao vive num [`Initializer`] do proprio Loco, e nao numa
//! funcao avulsa chamada em algum lugar: o framework ja' tem o gancho certo
//! para "rode antes de aceitar a primeira requisicao".

use async_trait::async_trait;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

/// Quantidade e janela de um limitador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimit {
    pub requests: u32,
    /// Janela em segundos.
    pub duration: u64,
}

impl RateLimit {
    pub const fn new(requests: u32, duration: u64) -> Self {
        Self { requests, duration }
    }
}

/// Os quatro limitadores nomeados da aplicacao.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimits {
    pub global: RateLimit,
    pub auth: RateLimit,
    pub strict: RateLimit,
    pub backup: RateLimit,
}

impl Default for RateLimits {
    fn default() -> Self {
        // Os mesmos numeros do Adonis. Ficam como default no codigo, e nao so'
        // no YAML, para que um arquivo de config incompleto nao afrouxe o
        // limite silenciosamente — afrouxar um rate limit por engano nao
        // quebra nada visivelmente, so' abre a porta.
        Self {
            global: RateLimit::new(600, 60),
            auth: RateLimit::new(5, 60),
            strict: RateLimit::new(60, 60),
            backup: RateLimit::new(60, 60),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Settings {
    /// Chave AES-256-GCM em hex (64 caracteres). Equivale a `DB_ENCRYPTION_KEY`.
    pub db_encryption_key: String,

    /// Token exigido para criar o primeiro admin. Vazio = sem exigencia fora
    /// de producao, exatamente como `hasValidBootstrapToken` faz.
    #[serde(default)]
    pub initial_admin_bootstrap_token: String,

    #[serde(default)]
    pub rate_limits: RateLimits,

    #[serde(default = "default_backup_storage_path")]
    pub backup_storage_path: String,

    #[serde(default = "default_diagnostics_path")]
    pub diagnostics_path: String,

    /// Dias de retencao dos logs de auditoria. `0` desliga a poda.
    #[serde(default = "default_audit_retention_days")]
    pub audit_retention_days: u32,

    /// Caminho no filesystem do `index.html` e assets da SPA (Fase 12.4).
    /// Em producao o frontend Vue e' construido e copiado para ca'.
    #[serde(default = "default_frontend_spa_path")]
    pub frontend_spa_path: String,

    /// Apagar a copia local depois de enviar o backup a um destino remoto?
    ///
    /// Equivale ao `BACKUP_DELETE_LOCAL_AFTER_REMOTE_UPLOAD`, e nasce
    /// **desligado** pelo mesmo motivo: manter as duas copias e' o que da' a'
    /// listagem a marca de replica e o que permite restaurar sem rede. Ligar
    /// economiza disco ao custo de depender do provedor para toda leitura.
    #[serde(default)]
    pub backup_delete_local_after_remote_upload: bool,
}

fn default_backup_storage_path() -> String {
    "/storage/backups".to_string()
}

fn default_diagnostics_path() -> String {
    "/storage/diagnostics".to_string()
}

const fn default_audit_retention_days() -> u32 {
    30
}

fn default_frontend_spa_path() -> String {
    "public".to_string()
}

impl Settings {
    /// Le o bloco `settings:` da config do Loco.
    ///
    /// Devolve `Err` — e nao um default silencioso — quando o bloco falta ou
    /// esta' malformado. Subir com uma chave de criptografia default seria
    /// pior que nao subir: os dados gravados ficariam ilegiveis para a chave
    /// de verdade.
    pub fn from_json(value: Option<&serde_json::Value>) -> Result<Self> {
        let raw = value.ok_or_else(|| {
            Error::Message(
                "o bloco `settings:` esta' ausente na configuracao; \
                 sem ele nao ha' DB_ENCRYPTION_KEY e nada que use criptografia funciona"
                    .to_string(),
            )
        })?;

        let settings: Self = serde_json::from_value(raw.clone())
            .map_err(|err| Error::Message(format!("bloco `settings:` invalido: {err}")))?;

        settings.validate()?;
        Ok(settings)
    }

    /// Confere o que so' da' para conferir depois da desserializacao.
    fn validate(&self) -> Result<()> {
        // A chave e' o item critico: 32 bytes em hex. Um valor com o tamanho
        // errado so' apareceria na primeira tentativa de descriptografar, que
        // pode ser semanas depois do deploy.
        if self.db_encryption_key.len() != 64 {
            return Err(Error::Message(format!(
                "db_encryption_key precisa ter 64 caracteres hex (32 bytes), tem {}",
                self.db_encryption_key.len()
            )));
        }

        if hex::decode(&self.db_encryption_key).is_err() {
            return Err(Error::Message(
                "db_encryption_key nao e' hexadecimal valido".to_string(),
            ));
        }

        Ok(())
    }

    /// Chave de criptografia como bytes, pronta para o `EncryptionService`.
    pub fn encryption_key_bytes(&self) -> Result<Vec<u8>> {
        hex::decode(&self.db_encryption_key)
            .map_err(|err| Error::Message(format!("db_encryption_key invalida: {err}")))
    }
}

/// Valida o bloco `settings:` durante o boot.
///
/// Nao guarda nada no contexto — o Loco nao tem um slot de estado de aplicacao
/// e inventar um `static` global contrariaria a inversao de dependencia via
/// `AppContext`. Quem precisa das settings chama [`Settings::from_json`] com
/// `ctx.config.settings.as_ref()`; este inicializador existe para garantir que
/// essa chamada nunca falhe em runtime por configuracao ausente.
pub struct SettingsInitializer;

#[async_trait]
impl Initializer for SettingsInitializer {
    fn name(&self) -> String {
        "settings".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        let settings = Settings::from_json(ctx.config.settings.as_ref())?;

        // Nunca logue a chave. O `Debug` da struct a renderia inteira, entao
        // aqui so' sai o que e' seguro conferir num log de boot.
        tracing::info!(
            global_limit = settings.rate_limits.global.requests,
            auth_limit = settings.rate_limits.auth.requests,
            "settings validadas"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    fn json(extra: &str) -> serde_json::Value {
        let body = if extra.is_empty() {
            String::new()
        } else {
            format!(",{extra}")
        };
        serde_json::from_str(&format!(
            r#"{{ "db_encryption_key": "{VALID_KEY}"{body} }}"#
        ))
        .unwrap()
    }

    #[test]
    fn reads_the_minimum_configuration() {
        let settings = Settings::from_json(Some(&json(""))).unwrap();

        assert_eq!(settings.db_encryption_key, VALID_KEY);
        assert_eq!(settings.audit_retention_days, 30);
    }

    #[test]
    fn defaults_match_the_adonis_limiters() {
        let settings = Settings::from_json(Some(&json(""))).unwrap();

        assert_eq!(settings.rate_limits.global, RateLimit::new(600, 60));
        assert_eq!(settings.rate_limits.auth, RateLimit::new(5, 60));
        assert_eq!(settings.rate_limits.strict, RateLimit::new(60, 60));
        assert_eq!(settings.rate_limits.backup, RateLimit::new(60, 60));
    }

    #[test]
    fn rejects_a_missing_settings_block() {
        let error = Settings::from_json(None).unwrap_err().to_string();
        assert!(error.contains("settings"), "mensagem pouco util: {error}");
    }

    #[test]
    fn rejects_a_key_of_the_wrong_length() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{ "db_encryption_key": "0011" }"#).unwrap();

        let error = Settings::from_json(Some(&value)).unwrap_err().to_string();
        assert!(error.contains("64"), "mensagem pouco util: {error}");
    }

    #[test]
    fn rejects_a_key_that_is_not_hex() {
        let value: serde_json::Value = serde_json::from_str(&format!(
            r#"{{ "db_encryption_key": "{}" }}"#,
            "z".repeat(64)
        ))
        .unwrap();

        assert!(Settings::from_json(Some(&value)).is_err());
    }

    #[test]
    fn decodes_the_encryption_key() {
        let settings = Settings::from_json(Some(&json(""))).unwrap();
        assert_eq!(settings.encryption_key_bytes().unwrap().len(), 32);
    }

    #[test]
    fn overrides_limits_from_the_yaml() {
        let value = json(r#""rate_limits": { "global": { "requests": 10, "duration": 1 } }"#);
        let error = Settings::from_json(Some(&value));

        // `RateLimits` tem `#[serde(default)]` no struct, entao um bloco
        // parcial preenche o resto com os defaults do Adonis em vez de falhar.
        let settings = error.unwrap();
        assert_eq!(settings.rate_limits.global, RateLimit::new(10, 1));
        assert_eq!(settings.rate_limits.auth, RateLimit::new(5, 60));
    }
}
