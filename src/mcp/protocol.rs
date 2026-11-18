//! Types JSON-RPC 2.0 et protocole MCP (Streamable HTTP).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: &str = "2025-03-26";
pub const SERVER_NAME: &str = "flux-de-fichiers";

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

pub const ERR_INVALID_REQUEST: i64 = -32600;
pub const ERR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERR_INVALID_PARAMS: i64 = -32602;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAnnotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<McpToolAnnotations>,
}

impl McpTool {
    pub const fn read_only(
        name: &'static str,
        description: &'static str,
        input_schema: Value,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            annotations: Some(McpToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                idempotent_hint: true,
            }),
        }
    }

    pub const fn write(
        name: &'static str,
        description: &'static str,
        input_schema: Value,
        idempotent_hint: bool,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            annotations: Some(McpToolAnnotations {
                read_only_hint: false,
                destructive_hint: false,
                idempotent_hint,
            }),
        }
    }

    pub const fn destructive(
        name: &'static str,
        description: &'static str,
        input_schema: Value,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            annotations: Some(McpToolAnnotations {
                read_only_hint: false,
                destructive_hint: true,
                idempotent_hint: false,
            }),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    pub is_error: bool,
}

#[derive(Debug, Serialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

impl McpToolResult {
    pub fn json<T: Serialize>(value: &T) -> Self {
        let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into());
        Self {
            content: vec![McpContent { kind: "text", text }],
            is_error: false,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![McpContent {
                kind: "text",
                text: message.into(),
            }],
            is_error: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    pub uri: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub mime_type: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpPromptArgument {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpPrompt {
    pub name: &'static str,
    pub description: &'static str,
    pub arguments: &'static [McpPromptArgument],
}

#[derive(Debug, Clone, Serialize)]
pub struct McpPromptMessage {
    pub role: &'static str,
    pub content: McpPromptText,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpPromptText {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

pub fn obj_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

pub fn empty_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

pub fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("paramètre requis manquant ou vide : {key}"))
}

pub fn parse_args<T: for<'de> Deserialize<'de>>(args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|err| format!("paramètres invalides : {err}"))
}
