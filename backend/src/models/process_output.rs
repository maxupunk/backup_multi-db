//! Captura limitada da saida de um processo filho.
//!
//! Porte de `app/services/process_output_buffer.ts`. O `stderr` de um
//! `mysqldump` ou de um `psql` e' a unica pista do que deu errado, mas nao pode
//! ser guardado sem teto: um `psql --echo-all` contra um dump grande produz
//! centenas de megabytes, e a mensagem de erro acabaria na coluna
//! `backups.error_message` — ou seja, dentro do banco de controle.
//!
//! O corte e' por **bytes**, e nao por caracteres, porque e' o que chega do
//! processo. A conversao para texto acontece uma unica vez, no final, com
//! `from_utf8_lossy`: cortar no meio de um caractere multibyte e' possivel, e
//! derrubar a mensagem de erro inteira por causa disso seria trocar um problema
//! por outro pior.
//!
//! ## Por que nao ha' porte de `child_process_exit.ts`
//!
//! Aquele helper existe porque, no Node, `spawn()` nao falha na hora: um
//! binario ausente vira um evento `error` assincrono, que corre com o evento
//! `close` e pode ser perdido. Em Rust, `tokio::process::Command::spawn()`
//! devolve `Err` imediatamente quando o binario nao existe, e `child.wait()`
//! devolve o codigo de saida. A corrida que o helper resolvia nao existe aqui.

/// Sufixo anexado quando algo foi descartado, igual ao do Adonis.
pub const TRUNCATION_SUFFIX: &str = "\n...[saida truncada pelo limite de captura]";

/// Teto default de captura: 256 KB, o mesmo que `BackupService` e
/// `RestoreService` usam.
pub const DEFAULT_LIMIT_BYTES: usize = 256 * 1024;

/// Acumula a saida de um processo ate' um teto, marcando se houve descarte.
#[derive(Debug)]
pub struct ProcessOutputBuffer {
    bytes: Vec<u8>,
    max_bytes: usize,
    truncated: bool,
}

impl Default for ProcessOutputBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_LIMIT_BYTES)
    }
}

impl ProcessOutputBuffer {
    #[must_use]
    pub fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
            truncated: false,
        }
    }

    /// Acrescenta um pedaco, guardando so' o que couber no teto.
    pub fn append(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }

        let remaining = self.max_bytes.saturating_sub(self.bytes.len());

        if remaining == 0 {
            self.truncated = true;
            return;
        }

        if chunk.len() <= remaining {
            self.bytes.extend_from_slice(chunk);
            return;
        }

        self.bytes.extend_from_slice(&chunk[..remaining]);
        self.truncated = true;
    }

    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Texto capturado, com o aviso de truncamento quando houve descarte.
    #[must_use]
    pub fn to_text(&self) -> String {
        let output = String::from_utf8_lossy(&self.bytes).into_owned();

        if !self.truncated {
            return output;
        }

        if output.is_empty() {
            return TRUNCATION_SUFFIX.trim_start().to_string();
        }

        if output.ends_with('\n') {
            format!("{output}{}", TRUNCATION_SUFFIX.trim_start())
        } else {
            format!("{output}{TRUNCATION_SUFFIX}")
        }
    }
}

/// Le' um `stderr`/`stdout` inteiro dentro do teto, sem alocar o processo todo.
///
/// Recebe qualquer `AsyncRead` para que o mesmo caminho sirva ao dump e ao
/// restore, e para que o teste possa passar um `&[u8]` sem levantar processo
/// nenhum.
pub async fn drain<R>(reader: R, max_bytes: usize) -> ProcessOutputBuffer
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut reader = reader;
    let mut buffer = ProcessOutputBuffer::new(max_bytes);
    let mut chunk = [0_u8; 8 * 1024];

    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => buffer.append(&chunk[..read]),
            // Um erro de leitura do pipe nao pode derrubar o backup: o que
            // interessa e' o codigo de saida do processo, e o que ja' foi
            // capturado continua sendo a melhor descricao disponivel.
            Err(_) => break,
        }
    }

    buffer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_everything_under_the_ceiling() {
        let mut buffer = ProcessOutputBuffer::new(64);
        buffer.append(b"ERROR 1045 (28000): Access denied\n");

        assert_eq!(buffer.to_text(), "ERROR 1045 (28000): Access denied\n");
        assert!(!buffer.is_truncated());
    }

    #[test]
    fn cuts_at_the_ceiling_and_says_so() {
        let mut buffer = ProcessOutputBuffer::new(5);
        buffer.append(b"0123456789");

        assert!(buffer.is_truncated());
        assert!(buffer.to_text().starts_with("01234"));
        assert!(buffer.to_text().contains("truncada"));
    }

    #[test]
    fn drops_whole_chunks_once_full() {
        // Continuar recebendo depois de cheio nao pode voltar a crescer: um
        // `psql` verboso emite pedacos indefinidamente.
        let mut buffer = ProcessOutputBuffer::new(4);
        buffer.append(b"abcd");
        buffer.append(b"efgh");
        buffer.append(b"ijkl");

        assert!(buffer.to_text().starts_with("abcd"));
        assert!(buffer.is_truncated());
    }

    #[test]
    fn does_not_duplicate_the_newline_before_the_notice() {
        let mut buffer = ProcessOutputBuffer::new(6);
        buffer.append(b"linha\nresto descartado");

        // Ja' termina em `\n`; o sufixo entra sem outra quebra.
        assert_eq!(
            buffer.to_text(),
            format!("linha\n{}", TRUNCATION_SUFFIX.trim_start())
        );
    }

    #[test]
    fn a_full_buffer_with_no_content_is_only_the_notice() {
        let mut buffer = ProcessOutputBuffer::new(0);
        buffer.append(b"qualquer coisa");

        assert_eq!(buffer.to_text(), TRUNCATION_SUFFIX.trim_start());
    }

    #[test]
    fn an_empty_chunk_changes_nothing() {
        let mut buffer = ProcessOutputBuffer::new(4);
        buffer.append(b"");

        assert!(buffer.is_empty());
        assert!(!buffer.is_truncated());
    }

    #[test]
    fn a_cut_in_the_middle_of_a_character_does_not_lose_the_message() {
        // `ç` ocupa dois bytes; cortar entre eles nao pode descartar a linha.
        let mut buffer = ProcessOutputBuffer::new(4);
        buffer.append("açao".as_bytes());

        let text = buffer.to_text();
        assert!(text.starts_with('a'), "perdeu o inicio da mensagem: {text}");
    }

    #[tokio::test]
    async fn drains_a_reader_up_to_the_ceiling() {
        let source = b"ERROR: relation \"clientes\" does not exist\n".to_vec();
        let buffer = drain(&source[..], DEFAULT_LIMIT_BYTES).await;

        assert_eq!(buffer.to_text(), String::from_utf8_lossy(&source));
        assert!(!buffer.is_truncated());
    }

    #[tokio::test]
    async fn drains_more_than_one_read_worth_of_output() {
        let source = vec![b'x'; 32 * 1024];
        let buffer = drain(&source[..], DEFAULT_LIMIT_BYTES).await;

        assert_eq!(buffer.to_text().len(), source.len());
    }
}
