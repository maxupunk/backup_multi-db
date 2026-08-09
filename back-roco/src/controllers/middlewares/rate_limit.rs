//! Rate limit compativel com o `@adonisjs/limiter` (tarefa 3.5 do roadmap).
//!
//! O Adonis usa store **em memoria** (`config/limiter.ts`), com quatro
//! limitadores nomeados e duas estrategias de chave. Este modulo reproduz o
//! algoritmo, os cabecalhos e o corpo do 429 — todos fixados por golden na
//! Fase 2 (`auth/login-rate-limited`).
//!
//! ## Por que janela fixa, e nao deslizante
//!
//! O `rate-limiter-flexible`, que o Adonis usa por baixo, implementa **janela
//! fixa**: o contador zera de uma vez quando a janela expira. Uma janela
//! deslizante seria mais justa, mas mudaria o comportamento observavel — o
//! teste de contrato faz 5 requisicoes e espera que a 6a seja bloqueada, e o
//! `Retry-After` de uma deslizante nao bateria com o gravado. Compatibilidade
//! vence elegancia aqui.
//!
//! ## Memoria
//!
//! Como no Adonis, o estado vive no processo e some no restart. A limpeza e'
//! preguicosa: entradas expiradas saem quando alguem passa por elas, mais uma
//! varredura quando o mapa cresce demais. Sem isso, um atacante variando o IP
//! faria o mapa crescer sem limite — que e' justamente o que um rate limiter
//! deveria impedir.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::initializers::settings::RateLimit;

/// Acima disto, uma varredura remove as entradas expiradas.
const CLEANUP_THRESHOLD: usize = 10_000;

/// Resultado de uma tentativa de consumo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub allowed: bool,
    pub limit: u32,
    /// Quantas requisicoes ainda cabem na janela. Nunca negativo.
    pub remaining: u32,
    /// Segundos ate' a janela zerar.
    pub retry_after: u64,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    hits: u32,
    expires_at: Instant,
}

/// Contador de janela fixa, chaveado por string.
#[derive(Debug)]
pub struct RateLimiter {
    entries: Mutex<HashMap<String, Entry>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Consome uma unidade de `key` sob o limite `config`.
    pub fn consume(&self, key: &str, config: RateLimit) -> Decision {
        self.consume_at(key, config, Instant::now())
    }

    /// Igual a `consume`, com o instante injetado — e' o que torna o
    /// comportamento de expiracao testavel sem `sleep`.
    pub fn consume_at(&self, key: &str, config: RateLimit, now: Instant) -> Decision {
        let window = Duration::from_secs(config.duration);
        let mut entries = self.entries.lock().unwrap_or_else(|poisoned| {
            // Um panic em outra thread nao pode desligar o rate limiter: seria
            // trocar um erro isolado por uma porta aberta.
            poisoned.into_inner()
        });

        if entries.len() > CLEANUP_THRESHOLD {
            entries.retain(|_, entry| entry.expires_at > now);
        }

        let entry = entries
            .entry(key.to_string())
            .and_modify(|existing| {
                if existing.expires_at <= now {
                    // Janela expirou: zera de uma vez, como o
                    // `rate-limiter-flexible` faz.
                    existing.hits = 0;
                    existing.expires_at = now + window;
                }
            })
            .or_insert(Entry {
                hits: 0,
                expires_at: now + window,
            });

        let retry_after = entry
            .expires_at
            .saturating_duration_since(now)
            .as_secs()
            .max(1);

        if entry.hits >= config.requests {
            // Ja' estourou: **nao** incrementa. Contar a requisicao bloqueada
            // estenderia o castigo a cada nova tentativa, o que o Adonis nao
            // faz — quem bate na porta durante o bloqueio nao deve prolonga-lo.
            return Decision {
                allowed: false,
                limit: config.requests,
                remaining: 0,
                retry_after,
            };
        }

        entry.hits += 1;

        Decision {
            allowed: true,
            limit: config.requests,
            remaining: config.requests.saturating_sub(entry.hits),
            retry_after,
        }
    }

    /// Quantas chaves estao sendo rastreadas. So' para teste e diagnostico.
    pub fn tracked_keys(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

/// Como a chave do limitador e' montada, espelhando `buildKey` do Adonis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStrategy {
    /// `<limiter>_<ip>`
    Ip,
    /// `<limiter>_<ip>_<email>` quando ha' e-mail no corpo; senao cai em `Ip`.
    IpEmail,
}

/// Monta a chave exatamente como `RateLimitMiddleware.buildKey`.
///
/// O e-mail entra **normalizado em minusculas e sem espacos**. Sem isso,
/// `Admin@X.com` e `admin@x.com ` teriam contadores separados e cinco
/// tentativas viraram quinze — o limitador de login deixaria de limitar.
pub fn build_key(limiter: &str, ip: &str, strategy: KeyStrategy, email: Option<&str>) -> String {
    match strategy {
        KeyStrategy::Ip => format!("{limiter}_{ip}"),
        KeyStrategy::IpEmail => match email.map(|value| value.trim().to_lowercase()) {
            Some(normalized) if !normalized.is_empty() => {
                format!("{limiter}_{ip}_{normalized}")
            }
            // Sem e-mail no corpo, o Adonis cai na chave so' de IP.
            _ => format!("{limiter}_{ip}"),
        },
    }
}

/// `X-RateLimit-Reset` no formato do Adonis: ISO-8601 em UTC com milissegundos.
pub fn reset_header(now: chrono::DateTime<chrono::Utc>, retry_after: u64) -> String {
    (now + chrono::Duration::seconds(retry_after as i64))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTH: RateLimit = RateLimit::new(5, 60);

    #[test]
    fn allows_up_to_the_limit_and_blocks_the_next() {
        let limiter = RateLimiter::new();

        // O contrato gravado na Fase 2: cinco passam, a sexta e' 429.
        for attempt in 1..=5 {
            let decision = limiter.consume("auth_1.2.3.4_a@b.c", AUTH);
            assert!(decision.allowed, "tentativa {attempt} foi bloqueada");
            assert_eq!(decision.remaining, 5 - attempt);
        }

        let blocked = limiter.consume("auth_1.2.3.4_a@b.c", AUTH);
        assert!(!blocked.allowed);
        assert_eq!(blocked.remaining, 0);
        assert_eq!(blocked.limit, 5);
    }

    #[test]
    fn a_blocked_request_does_not_extend_the_punishment() {
        let limiter = RateLimiter::new();
        let start = Instant::now();

        for _ in 0..5 {
            limiter.consume_at("k", AUTH, start);
        }

        // Trinta segundos depois, ainda bloqueado — mas o `retry_after`
        // continua contando para o fim da janela original, e nao reinicia a
        // cada batida na porta.
        let primeiro = limiter.consume_at("k", AUTH, start + Duration::from_secs(30));
        let segundo = limiter.consume_at("k", AUTH, start + Duration::from_secs(30));

        assert!(!primeiro.allowed && !segundo.allowed);
        assert_eq!(primeiro.retry_after, segundo.retry_after);
        assert_eq!(primeiro.retry_after, 30);
    }

    #[test]
    fn the_window_resets_all_at_once() {
        let limiter = RateLimiter::new();
        let start = Instant::now();

        for _ in 0..5 {
            limiter.consume_at("k", AUTH, start);
        }
        assert!(!limiter.consume_at("k", AUTH, start).allowed);

        // Janela fixa: passados os 60s o contador zera inteiro, e nao aos
        // poucos como numa deslizante.
        let depois = limiter.consume_at("k", AUTH, start + Duration::from_secs(61));
        assert!(depois.allowed);
        assert_eq!(depois.remaining, 4);
    }

    #[test]
    fn keys_are_independent() {
        let limiter = RateLimiter::new();

        for _ in 0..5 {
            limiter.consume("auth_1.1.1.1_a@b.c", AUTH);
        }

        // Outro e-mail no mesmo IP tem contador proprio — e' o que permite a
        // suite de contrato testar 429 sem derrubar os testes vizinhos.
        assert!(limiter.consume("auth_1.1.1.1_outro@b.c", AUTH).allowed);
        assert!(limiter.consume("auth_2.2.2.2_a@b.c", AUTH).allowed);
    }

    #[test]
    fn retry_after_is_never_zero() {
        // Um `Retry-After: 0` diria ao cliente para tentar imediatamente, o que
        // renderia outro 429 na hora.
        let limiter = RateLimiter::new();
        let start = Instant::now();

        for _ in 0..5 {
            limiter.consume_at("k", AUTH, start);
        }

        let quase_no_fim = limiter.consume_at("k", AUTH, start + Duration::from_millis(59_999));
        assert!(quase_no_fim.retry_after >= 1);
    }

    #[test]
    fn builds_the_ip_key() {
        assert_eq!(
            build_key("global", "127.0.0.1", KeyStrategy::Ip, Some("a@b.c")),
            "global_127.0.0.1"
        );
    }

    #[test]
    fn builds_the_ip_email_key() {
        assert_eq!(
            build_key("auth", "127.0.0.1", KeyStrategy::IpEmail, Some("a@b.c")),
            "auth_127.0.0.1_a@b.c"
        );
    }

    #[test]
    fn normalizes_the_email_in_the_key() {
        // Sem normalizar, `Admin@X.com` e `admin@x.com` teriam contadores
        // separados e o limitador de login deixaria de limitar.
        let esperado = "auth_127.0.0.1_admin@x.com";

        for variacao in ["admin@x.com", "Admin@X.com", "  ADMIN@x.com  "] {
            assert_eq!(
                build_key("auth", "127.0.0.1", KeyStrategy::IpEmail, Some(variacao)),
                esperado,
                "falhou em {variacao:?}"
            );
        }
    }

    #[test]
    fn falls_back_to_ip_when_there_is_no_email() {
        for email in [None, Some(""), Some("   ")] {
            assert_eq!(
                build_key("auth", "127.0.0.1", KeyStrategy::IpEmail, email),
                "auth_127.0.0.1",
                "falhou em {email:?}"
            );
        }
    }

    #[test]
    fn formats_the_reset_header_like_adonis() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-09T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        assert_eq!(reset_header(now, 60), "2026-08-09T12:01:00.000Z");
    }

    #[test]
    fn cleans_up_expired_entries_instead_of_growing_forever() {
        let limiter = RateLimiter::new();
        let start = Instant::now();

        for index in 0..=CLEANUP_THRESHOLD {
            limiter.consume_at(&format!("k{index}"), AUTH, start);
        }

        // Depois da janela, a proxima chamada dispara a varredura e o mapa
        // encolhe. Sem isso, variar o IP faria o limitador virar um vazamento
        // de memoria — exatamente o que ele deveria impedir.
        limiter.consume_at("gatilho", AUTH, start + Duration::from_secs(61));
        assert!(limiter.tracked_keys() < CLEANUP_THRESHOLD);
    }
}
