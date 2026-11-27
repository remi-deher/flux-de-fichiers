//! Cadrage des messages du protocole de langage sur stdio : des en-têtes, puis un corps JSON dont
//! `Content-Length` donne la taille en octets.

use serde_json::Value;
use std::io;
use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Taille maximale d'un message du serveur de langage (ADR 0032, point 4).
pub const MAX_SERVER_MESSAGE_BYTES: usize = 64 * 1024;
/// Au-delà, un message trop gros n'est plus sauté mais coupe le flux.
const MAX_SKIPPED_BYTES: usize = 4 * 1024 * 1024;
/// Longueur maximale d'une ligne d'en-tête : le protocole n'en a que de courtes.
const MAX_HEADER_LINE_BYTES: u64 = 256;
const LENGTH_HEADER: &str = "content-length";

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("lecture ou écriture impossible : {0}")]
    Io(#[from] io::Error),
    #[error("en-tête illisible")]
    MalformedHeader,
    #[error("longueur du message absente")]
    MissingLength,
    /// Message plus gros que le plafond : il a été lu et jeté, le flux reste aligné sur le suivant.
    #[error("message de {0} octets jeté : au-delà du plafond")]
    Oversized(usize),
    #[error("message illisible : {0}")]
    InvalidJson(#[from] serde_json::Error),
}

impl CodecError {
    /// Le flux est-il encore utilisable après cette erreur ?
    pub const fn is_recoverable(&self) -> bool {
        matches!(self, Self::Oversized(_) | Self::InvalidJson(_))
    }
}

/// Lit les en-têtes d'un message et rend la longueur annoncée de son corps ; `None` à la fin du flux
/// entre deux messages.
async fn read_length<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<usize>, CodecError> {
    let mut length = None;
    let mut started = false;
    loop {
        let mut line = String::new();
        let mut limited = (&mut *reader).take(MAX_HEADER_LINE_BYTES);
        let read = limited.read_line(&mut line).await?;
        if read == 0 && !started {
            return Ok(None);
        }
        started = true;
        if !line.ends_with('\n') {
            return Err(CodecError::MalformedHeader);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            return length.map(Some).ok_or(CodecError::MissingLength);
        }
        let (name, value) = header.split_once(':').ok_or(CodecError::MalformedHeader)?;
        if name.trim().eq_ignore_ascii_case(LENGTH_HEADER) {
            let parsed = value
                .trim()
                .parse()
                .map_err(|_| CodecError::MalformedHeader)?;
            length = Some(parsed);
        }
    }
}

/// Lit un message ; `None` à la fin du flux entre deux messages. Un message de plus de `max_bytes` est
/// lu et jeté (`Oversized`), sauf s'il dépasse de beaucoup le plafond : le flux est alors coupé.
pub async fn read_message<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<Option<Value>, CodecError> {
    let Some(length) = read_length(reader).await? else {
        return Ok(None);
    };
    if length > max_bytes {
        if length > MAX_SKIPPED_BYTES {
            return Err(CodecError::MalformedHeader);
        }
        let mut body = (&mut *reader).take(length as u64);
        let skipped = tokio::io::copy(&mut body, &mut tokio::io::sink()).await?;
        if skipped != length as u64 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
        return Err(CodecError::Oversized(length));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}

/// Écrit un message avec son en-tête, puis vide le tampon.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Value,
) -> Result<(), CodecError> {
    let body = serde_json::to_vec(message)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ROOMY: usize = 1024;

    fn framed(body: &str) -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
    }

    #[tokio::test]
    async fn a_framed_message_is_read() -> Result<(), CodecError> {
        let bytes = framed(r#"{"jsonrpc":"2.0","id":1}"#);
        let mut reader = &bytes[..];
        let message = read_message(&mut reader, ROOMY).await?;
        assert_eq!(message, Some(json!({"jsonrpc": "2.0", "id": 1})));
        Ok(())
    }

    #[tokio::test]
    async fn consecutive_messages_are_read_in_order() -> Result<(), CodecError> {
        let mut bytes = framed(r#"{"n":1}"#);
        bytes.extend(framed(r#"{"n":2}"#));
        let mut reader = &bytes[..];
        let first = read_message(&mut reader, ROOMY).await?;
        let second = read_message(&mut reader, ROOMY).await?;
        let end = read_message(&mut reader, ROOMY).await?;
        assert_eq!(first, Some(json!({"n": 1})));
        assert_eq!(second, Some(json!({"n": 2})));
        assert_eq!(end, None);
        Ok(())
    }

    #[tokio::test]
    async fn the_length_header_is_case_insensitive() -> Result<(), CodecError> {
        let bytes = b"content-type: text/plain\r\nCONTENT-LENGTH: 2\r\n\r\n{}";
        let mut reader = &bytes[..];
        let message = read_message(&mut reader, ROOMY).await?;
        assert_eq!(message, Some(json!({})));
        Ok(())
    }

    #[tokio::test]
    async fn the_length_counts_bytes_not_characters() -> Result<(), CodecError> {
        let bytes = framed(r#"{"texte":"é"}"#);
        let mut reader = &bytes[..];
        let message = read_message(&mut reader, ROOMY).await?;
        assert_eq!(message, Some(json!({"texte": "é"})));
        Ok(())
    }

    #[tokio::test]
    async fn a_message_without_a_length_is_refused() {
        let mut reader = &b"Content-Type: x\r\n\r\n{}"[..];
        let result = read_message(&mut reader, ROOMY).await;
        assert!(matches!(result, Err(CodecError::MissingLength)));
    }

    #[tokio::test]
    async fn a_length_that_is_not_a_number_is_refused() {
        let mut reader = &b"Content-Length: beaucoup\r\n\r\n{}"[..];
        let result = read_message(&mut reader, ROOMY).await;
        assert!(matches!(result, Err(CodecError::MalformedHeader)));
    }

    #[tokio::test]
    async fn a_header_line_without_a_colon_or_too_long_is_refused() {
        let mut reader = &b"Content-Length 2\r\n\r\n{}"[..];
        let result = read_message(&mut reader, ROOMY).await;
        assert!(matches!(result, Err(CodecError::MalformedHeader)));
        let long = format!("X: {}\r\n\r\n", "a".repeat(1000));
        let mut reader = long.as_bytes();
        let result = read_message(&mut reader, ROOMY).await;
        assert!(matches!(result, Err(CodecError::MalformedHeader)));
    }

    #[tokio::test]
    async fn a_message_over_the_ceiling_is_skipped() -> Result<(), CodecError> {
        let mut bytes = framed(&format!(r#"{{"gros":"{}"}}"#, "x".repeat(200)));
        bytes.extend(framed(r#"{"petit":true}"#));
        let mut reader = &bytes[..];
        let first = read_message(&mut reader, 100).await;
        assert!(matches!(first, Err(CodecError::Oversized(_))));
        assert!(first.is_err_and(|error| error.is_recoverable()));
        let second = read_message(&mut reader, 100).await?;
        assert_eq!(second, Some(json!({"petit": true})));
        Ok(())
    }

    #[tokio::test]
    async fn a_message_far_over_the_ceiling_cuts_the_stream() {
        let header = format!("Content-Length: {}\r\n\r\n", MAX_SKIPPED_BYTES + 1);
        let mut reader = header.as_bytes();
        let result = read_message(&mut reader, 100).await;
        assert!(result.is_err_and(|error| !error.is_recoverable()));
    }

    #[tokio::test]
    async fn a_truncated_body_is_an_error() {
        let mut reader = &b"Content-Length: 50\r\n\r\n{}"[..];
        let result = read_message(&mut reader, ROOMY).await;
        assert!(matches!(result, Err(CodecError::Io(_))));
    }

    #[tokio::test]
    async fn a_body_that_is_not_json_is_recoverable() {
        let bytes = framed("pas du json");
        let mut reader = &bytes[..];
        let result = read_message(&mut reader, ROOMY).await;
        assert!(result.is_err_and(|error| error.is_recoverable()));
    }

    #[tokio::test]
    async fn a_written_message_is_read_back() -> Result<(), CodecError> {
        let message = json!({"method": "initialized", "params": {}});
        let mut written = Vec::new();
        write_message(&mut written, &message).await?;
        assert!(written.starts_with(b"Content-Length: "));
        let mut reader = &written[..];
        assert_eq!(read_message(&mut reader, ROOMY).await?, Some(message));
        Ok(())
    }
}
