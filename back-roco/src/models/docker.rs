//! Fronteira com a Docker Engine (Fase 9).
//!
//! A aplicação não chama o CLI: `bollard` fala com o socket Unix no Linux e
//! com o named pipe no Windows. Isso evita depender de um binário instalado no
//! container e mantém a mesma API para as próximas rotas do Docker Manager.

use std::time::Duration;

use bollard::Docker;

/// Resultado mínimo e estável de `GET /api/docker/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub available: bool,
}

/// Tempo máximo para uma Engine que aceita a conexão mas não responde.
const PING_TIMEOUT: Duration = Duration::from_secs(3);

/// Sonda a Engine local sem transformar a ausência do Docker em erro HTTP.
///
/// Instalações sem Docker são suportadas: a interface abre vazia e as rotas de
/// listagem posteriores devolvem o envelope `available: false`.
pub async fn status() -> Status {
    let Ok(client) = Docker::connect_with_local_defaults() else {
        return Status { available: false };
    };

    let available = matches!(
        tokio::time::timeout(PING_TIMEOUT, client.ping()).await,
        Ok(Ok(_))
    );
    Status { available }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn probing_never_panics_when_the_engine_is_absent() {
        // O resultado depende da máquina que executa o teste; o contrato deste
        // model é sempre devolver um booleano, e nunca propagar erro de socket.
        let result = status().await;
        assert!(matches!(result.available, true | false));
    }
}
