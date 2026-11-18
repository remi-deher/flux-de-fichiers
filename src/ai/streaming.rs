//! Lecture progressive des réponses des fournisseurs IA : le texte est transmis au fur et à mesure.
//!
//! Chaque fournisseur a son format de flux (SSE OpenAI, SSE Anthropic, JSON ligne par ligne
//! d'Ollama, SSE « responses » de ChatGPT). Un assembleur le relit ligne à ligne et reconstitue le
//! corps JSON qu'une réponse non diffusée aurait eu : l'analyse des appels d'outils reste unique,
//! dans `provider.rs`.

use super::provider::truncate_error;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

/// Réception du texte au fil de l'eau ; `None` désactive la diffusion.
pub type DeltaSink<'a> = Option<&'a (dyn Fn(&str) + Send + Sync)>;

const SSE_DATA_PREFIX: &str = "data:";
const SSE_DONE: &str = "[DONE]";
/// Texte regroupé avant d'être transmis, pour ne pas publier un évènement par jeton.
const FLUSH_CHARS: usize = 24;
const FLUSH_INTERVAL: Duration = Duration::from_millis(60);

/// Contenu d'une ligne `data:` d'un flux SSE.
fn sse_data(line: &str) -> Option<&str> {
    line.trim().strip_prefix(SSE_DATA_PREFIX).map(str::trim)
}

fn object_or_empty(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

/// Reconstitue un corps de réponse à partir des lignes d'un flux.
pub trait StreamAssembler: Send {
    /// Traite une ligne ; renvoie le texte de réponse qu'elle apporte, s'il y en a.
    fn push_line(&mut self, line: &str) -> Result<Option<String>, String>;
    /// Corps équivalent à la réponse non diffusée.
    fn finish(self) -> Result<Value, String>;
}

/// OpenAI et API compatibles (`chat/completions` avec `stream: true`).
#[derive(Default)]
pub struct OpenAiStream {
    text: String,
    /// Appels d'outils par indice : (identifiant, nom, arguments JSON en cours d'écriture).
    calls: BTreeMap<u64, (String, String, String)>,
}

impl StreamAssembler for OpenAiStream {
    fn push_line(&mut self, line: &str) -> Result<Option<String>, String> {
        let Some(data) = sse_data(line).filter(|data| *data != SSE_DONE) else {
            return Ok(None);
        };
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            return Ok(None);
        };
        if let Some(error) = chunk.get("error") {
            return Err(format!(
                "Le fournisseur a interrompu la réponse : {}",
                truncate_error(&error.to_string())
            ));
        }
        let Some(delta) = chunk.pointer("/choices/0/delta") else {
            return Ok(None);
        };
        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
            let entry = self.calls.entry(index).or_default();
            if let Some(id) = call.get("id").and_then(Value::as_str) {
                entry.0 = id.to_owned();
            }
            if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                entry.1.push_str(name);
            }
            if let Some(arguments) = call.pointer("/function/arguments").and_then(Value::as_str) {
                entry.2.push_str(arguments);
            }
        }
        let text = delta
            .get("content")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty());
        if let Some(text) = text {
            self.text.push_str(text);
        }
        Ok(text.map(ToOwned::to_owned))
    }

    fn finish(self) -> Result<Value, String> {
        let mut message = json!({ "role": "assistant", "content": self.text });
        if !self.calls.is_empty() {
            message["tool_calls"] = self
                .calls
                .into_values()
                .map(|(id, name, arguments)| {
                    let mut call = json!({
                        "type": "function",
                        "function": { "name": name, "arguments": arguments },
                    });
                    if !id.is_empty() {
                        call["id"] = Value::String(id);
                    }
                    call
                })
                .collect();
        }
        Ok(json!({ "choices": [{ "message": message }] }))
    }
}

/// Ollama (`/api/chat` avec `stream: true`) : un objet JSON par ligne.
#[derive(Default)]
pub struct OllamaStream {
    text: String,
    calls: Vec<Value>,
}

impl StreamAssembler for OllamaStream {
    fn push_line(&mut self, line: &str) -> Result<Option<String>, String> {
        let Ok(chunk) = serde_json::from_str::<Value>(line.trim()) else {
            return Ok(None);
        };
        if let Some(error) = chunk.get("error") {
            return Err(format!(
                "Ollama a interrompu la réponse : {}",
                truncate_error(&error.to_string())
            ));
        }
        let message = chunk.get("message").unwrap_or(&Value::Null);
        if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
            self.calls.extend(calls.iter().cloned());
        }
        let text = message
            .get("content")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty());
        if let Some(text) = text {
            self.text.push_str(text);
        }
        Ok(text.map(ToOwned::to_owned))
    }

    fn finish(self) -> Result<Value, String> {
        Ok(json!({ "message": { "content": self.text, "tool_calls": self.calls } }))
    }
}

/// Bloc de contenu d'Anthropic en cours de lecture.
enum AnthropicBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input_json: String,
    },
}

/// Anthropic (`messages` avec `stream: true`).
#[derive(Default)]
pub struct AnthropicStream {
    blocks: BTreeMap<u64, AnthropicBlock>,
}

impl StreamAssembler for AnthropicStream {
    fn push_line(&mut self, line: &str) -> Result<Option<String>, String> {
        let Some(data) = sse_data(line) else {
            return Ok(None);
        };
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return Ok(None);
        };
        let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
        match event.get("type").and_then(Value::as_str) {
            Some("content_block_start") => {
                let block = event.get("content_block").unwrap_or(&Value::Null);
                let started = match block.get("type").and_then(Value::as_str) {
                    Some("tool_use") => AnthropicBlock::ToolUse {
                        id: block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        name: block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        input_json: String::new(),
                    },
                    _ => AnthropicBlock::Text(String::new()),
                };
                self.blocks.insert(index, started);
                Ok(None)
            }
            Some("content_block_delta") => {
                let delta = event.get("delta").unwrap_or(&Value::Null);
                match (
                    self.blocks.get_mut(&index),
                    delta.get("type").and_then(Value::as_str),
                ) {
                    (Some(AnthropicBlock::Text(text)), Some("text_delta")) => {
                        let piece = delta
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        text.push_str(piece);
                        Ok((!piece.is_empty()).then(|| piece.to_owned()))
                    }
                    (
                        Some(AnthropicBlock::ToolUse { input_json, .. }),
                        Some("input_json_delta"),
                    ) => {
                        input_json.push_str(
                            delta
                                .get("partial_json")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                        );
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }
            Some("error") => Err(format!(
                "Anthropic a interrompu la réponse : {}",
                truncate_error(&event.to_string())
            )),
            _ => Ok(None),
        }
    }

    fn finish(self) -> Result<Value, String> {
        let content: Vec<Value> = self
            .blocks
            .into_values()
            .map(|block| match block {
                AnthropicBlock::Text(text) => json!({ "type": "text", "text": text }),
                AnthropicBlock::ToolUse {
                    id,
                    name,
                    input_json,
                } => json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": object_or_empty(&input_json),
                }),
            })
            .collect();
        Ok(json!({ "content": content }))
    }
}

/// ChatGPT (`responses`) : le corps complet est gardé tel quel pour `parse_responses_sse`.
#[derive(Default)]
pub struct ResponsesStream {
    body: String,
}

impl StreamAssembler for ResponsesStream {
    fn push_line(&mut self, line: &str) -> Result<Option<String>, String> {
        self.body.push_str(line);
        self.body.push('\n');
        let Some(event) = sse_data(line).and_then(|data| serde_json::from_str::<Value>(data).ok())
        else {
            return Ok(None);
        };
        let text = (event.get("type").and_then(Value::as_str)
            == Some("response.output_text.delta"))
        .then(|| event.get("delta").and_then(Value::as_str))
        .flatten()
        .filter(|text| !text.is_empty());
        Ok(text.map(ToOwned::to_owned))
    }

    fn finish(self) -> Result<Value, String> {
        Ok(Value::String(self.body))
    }
}

/// Découpe un flux d'octets en lignes complètes, sans casser un caractère UTF-8 entre deux morceaux.
#[derive(Default)]
struct LineSplitter {
    pending: Vec<u8>,
}

impl LineSplitter {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            lines.push(String::from_utf8_lossy(&line).trim_end().to_owned());
        }
        lines
    }

    /// Dernière ligne, si le flux ne s'est pas terminé par un saut de ligne.
    fn rest(self) -> Option<String> {
        let rest = String::from_utf8_lossy(&self.pending).trim().to_owned();
        (!rest.is_empty()).then_some(rest)
    }
}

/// Regroupe le texte avant de le transmettre.
struct Coalescer<'a> {
    sink: DeltaSink<'a>,
    pending: String,
    last_flush: Instant,
}

impl<'a> Coalescer<'a> {
    fn new(sink: DeltaSink<'a>) -> Self {
        Self {
            sink,
            pending: String::new(),
            last_flush: Instant::now(),
        }
    }

    fn push(&mut self, text: &str) {
        self.pending.push_str(text);
        if self.pending.chars().count() >= FLUSH_CHARS
            || self.last_flush.elapsed() >= FLUSH_INTERVAL
        {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if let (Some(sink), false) = (self.sink, self.pending.is_empty()) {
            sink(&self.pending);
        }
        self.pending.clear();
        self.last_flush = Instant::now();
    }
}

/// Lit la réponse : un flux est assemblé (et son texte transmis à `on_delta`), sinon le corps JSON
/// est lu d'un bloc. Une réponse en erreur est toujours lue en entier pour en citer le message.
pub async fn read_response<A: StreamAssembler>(
    response: reqwest::Response,
    http_label: &str,
    invalid_label: &str,
    assembler: Option<A>,
    on_delta: DeltaSink<'_>,
) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!(
            "{http_label} HTTP {} : {}",
            status.as_u16(),
            truncate_error(&text)
        ));
    }
    // Un serveur qui ignore `stream` répond en JSON classique : on le lit d'un bloc.
    let plain_json = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.starts_with("application/json"));
    let Some(mut assembler) = assembler.filter(|_| !plain_json) else {
        return response
            .json::<Value>()
            .await
            .map_err(|err| format!("{invalid_label} : {}", truncate_error(&err.to_string())));
    };
    let mut splitter = LineSplitter::default();
    let mut coalescer = Coalescer::new(on_delta);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|err| format!("{invalid_label} : {}", truncate_error(&err.to_string())))?;
        for line in splitter.push(&chunk) {
            if let Some(text) = assembler.push_line(&line)? {
                coalescer.push(&text);
            }
        }
    }
    if let Some(line) = splitter.rest()
        && let Some(text) = assembler.push_line(&line)?
    {
        coalescer.push(&text);
    }
    coalescer.flush();
    assembler.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn feed<A: StreamAssembler>(mut assembler: A, lines: &[&str]) -> (Vec<String>, Value) {
        let mut deltas = Vec::new();
        for line in lines {
            if let Ok(Some(text)) = assembler.push_line(line) {
                deltas.push(text);
            }
        }
        (deltas, assembler.finish().unwrap_or_else(|_| json!({})))
    }

    #[test]
    fn openai_chunks_become_text_deltas_and_a_complete_tool_call() {
        let (deltas, body) = feed(
            OpenAiStream::default(),
            &[
                ": keep-alive",
                r#"data: {"choices":[{"delta":{"role":"assistant","content":"Je "}}]}"#,
                r#"data: {"choices":[{"delta":{"content":"regarde"}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"list_jobs","arguments":""}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"limit\":"}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"5}"}}]}}]}"#,
                "data: [DONE]",
            ],
        );
        assert_eq!(deltas, ["Je ", "regarde"]);
        let message = &body["choices"][0]["message"];
        assert_eq!(message["content"], "Je regarde");
        assert_eq!(message["tool_calls"][0]["id"], "call_1");
        assert_eq!(message["tool_calls"][0]["function"]["name"], "list_jobs");
        assert_eq!(
            message["tool_calls"][0]["function"]["arguments"],
            "{\"limit\":5}"
        );
    }

    #[test]
    fn an_openai_error_chunk_stops_the_stream() {
        let mut assembler = OpenAiStream::default();
        let error = assembler
            .push_line(r#"data: {"error":{"message":"quota"}}"#)
            .err()
            .unwrap_or_default();
        assert!(error.contains("quota"));
    }

    #[test]
    fn ollama_lines_are_joined_and_keep_their_tool_calls() {
        let (deltas, body) = feed(
            OllamaStream::default(),
            &[
                r#"{"message":{"role":"assistant","content":"Bon"},"done":false}"#,
                r#"{"message":{"role":"assistant","content":"jour"},"done":false}"#,
                r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"list_workers","arguments":{}}}]},"done":true}"#,
            ],
        );
        assert_eq!(deltas, ["Bon", "jour"]);
        assert_eq!(body["message"]["content"], "Bonjour");
        assert_eq!(
            body["message"]["tool_calls"][0]["function"]["name"],
            "list_workers"
        );
    }

    #[test]
    fn anthropic_events_rebuild_text_and_tool_blocks_in_order() {
        let (deltas, body) = feed(
            AnthropicStream::default(),
            &[
                "event: content_block_start",
                r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
                r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Voyons"}}"#,
                r#"data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu_1","name":"get_job","input":{}}}"#,
                r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"id\":"}}"#,
                r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"j1\"}"}}"#,
                r#"data: {"type":"message_stop"}"#,
            ],
        );
        assert_eq!(deltas, ["Voyons"]);
        assert_eq!(body["content"][0]["text"], "Voyons");
        assert_eq!(body["content"][1]["name"], "get_job");
        assert_eq!(body["content"][1]["input"]["id"], "j1");
    }

    #[test]
    fn chatgpt_text_deltas_are_streamed_while_the_body_is_kept_whole() {
        let lines = [
            r#"data: {"type":"response.output_text.delta","delta":"Bonjour"}"#,
            r#"data: {"type":"response.completed"}"#,
        ];
        let (deltas, body) = feed(ResponsesStream::default(), &lines);
        assert_eq!(deltas, ["Bonjour"]);
        let kept = body.as_str().unwrap_or_default();
        assert!(kept.contains("response.completed") && kept.contains("Bonjour"));
    }

    #[test]
    fn lines_are_rebuilt_across_chunks_even_inside_a_multibyte_character() {
        let text = "data: é\r\ndata: suite\n".as_bytes();
        let cut = text.iter().position(|b| *b == 0xC3).map_or(0, |at| at + 1);
        let mut splitter = LineSplitter::default();
        assert!(splitter.push(&text[..cut]).is_empty());
        assert_eq!(splitter.push(&text[cut..]), ["data: é", "data: suite"]);
        let mut tail = LineSplitter::default();
        assert!(tail.push(b"data: fin").is_empty());
        assert_eq!(tail.rest().as_deref(), Some("data: fin"));
    }

    #[test]
    fn small_deltas_are_grouped_and_nothing_is_lost_at_the_end() {
        let received = Mutex::new(Vec::<String>::new());
        let sink = |text: &str| {
            if let Ok(mut all) = received.lock() {
                all.push(text.to_owned());
            }
        };
        let mut coalescer = Coalescer::new(Some(&sink));
        for _ in 0..30 {
            coalescer.push("ab");
        }
        coalescer.flush();
        let all = received.lock().map(|all| all.clone()).unwrap_or_default();
        assert!(all.len() < 30, "regroupés : {all:?}");
        assert_eq!(all.concat(), "ab".repeat(30));
        // Sans destinataire, rien ne plante.
        let mut silent = Coalescer::new(None);
        silent.push("x");
        silent.flush();
    }

    #[tokio::test]
    async fn read_response_assembles_a_stream_and_reports_error_statuses() -> anyhow::Result<()> {
        use axum::http::Response;
        let chunks = [
            "data: {\"choices\":[{\"delta\":{\"content\":\"Bon\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"jour\"}}]}\n\ndata: [DONE]\n",
        ];
        let body = reqwest::Body::wrap_stream(futures_util::stream::iter(
            chunks.map(|chunk| Ok::<_, std::io::Error>(chunk.as_bytes().to_vec())),
        ));
        let response = reqwest::Response::from(Response::builder().status(200).body(body)?);
        let received = Mutex::new(String::new());
        let sink = |text: &str| {
            if let Ok(mut all) = received.lock() {
                all.push_str(text);
            }
        };
        let value = read_response(
            response,
            "Fournisseur",
            "Réponse invalide",
            Some(OpenAiStream::default()),
            Some(&sink),
        )
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
        assert_eq!(value["choices"][0]["message"]["content"], "Bonjour");
        assert_eq!(
            received.lock().map(|r| r.clone()).unwrap_or_default(),
            "Bonjour"
        );

        let failing = reqwest::Response::from(
            Response::builder()
                .status(429)
                .body("{\"error\":\"trop\"}")?,
        );
        let error = read_response::<OpenAiStream>(failing, "Fournisseur", "x", None, None)
            .await
            .err()
            .unwrap_or_default();
        assert!(error.starts_with("Fournisseur HTTP 429") && error.contains("trop"));

        let plain = reqwest::Response::from(Response::builder().status(200).body("{\"ok\":true}")?);
        let value = read_response::<OpenAiStream>(plain, "F", "x", None, None)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        assert_eq!(value["ok"], true);
        Ok(())
    }
}
