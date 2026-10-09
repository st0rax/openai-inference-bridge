//! Request validation and normalization for non-streaming Chat Completions.

use crate::{
    api_error::ApiError,
    json::{self, JsonValue},
};

pub const MAX_CHAT_BODY_BYTES: usize = 1024 * 1024;
const MAX_MESSAGES: usize = 128;
const MAX_MODEL_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl ChatRole {
    fn prompt_label(self) -> &'static str {
        match self {
            Self::System => "System instruction",
            Self::User => "User",
            Self::Assistant => "Assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedChatRequest {
    pub model_id: String,
    pub messages: Vec<ChatMessage>,
}

impl NormalizedChatRequest {
    /// Preserve every message in order when converting to a single browser prompt.
    pub fn compose_prompt(&self) -> String {
        let mut prompt = String::new();
        for message in &self.messages {
            prompt.push_str(message.role.prompt_label());
            prompt.push_str(":\n");
            prompt.push_str(&message.content);
            prompt.push_str("\n\n");
        }
        prompt.push_str("Respond to the final user message.");
        prompt
    }
}

pub fn normalize_request(body: &[u8]) -> Result<NormalizedChatRequest, ApiError> {
    if body.len() > MAX_CHAT_BODY_BYTES {
        return Err(ApiError::new(
            413,
            "invalid_request_error",
            "Request body exceeds the 1 MiB limit",
        )
        .with_code("request_too_large"));
    }

    let input = std::str::from_utf8(body).map_err(|_| {
        ApiError::new(
            400,
            "invalid_request_error",
            "Request body must be UTF-8 JSON",
        )
        .with_code("invalid_json")
    })?;
    let value = json::parse(input).map_err(|_| {
        ApiError::new(
            400,
            "invalid_request_error",
            "Request body must be valid JSON",
        )
        .with_code("invalid_json")
    })?;
    let object = value.as_object().ok_or_else(|| {
        ApiError::new(
            400,
            "invalid_request_error",
            "Request body must be a JSON object",
        )
        .with_code("invalid_request")
    })?;

    for key in object.keys() {
        if !matches!(key.as_str(), "model" | "messages" | "stream") {
            return Err(ApiError::new(
                400,
                "invalid_request_error",
                "Unsupported field in Chat Completions request",
            )
            .with_param(key)
            .with_code("unsupported_parameter"));
        }
    }

    let model_id = object
        .get("model")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_MODEL_ID_BYTES)
        .ok_or_else(|| {
            ApiError::new(
                400,
                "invalid_request_error",
                "A non-empty model ID is required",
            )
            .with_param("model")
            .with_code("invalid_model")
        })?
        .to_owned();

    if let Some(stream) = object.get("stream") {
        match stream.as_bool() {
            Some(false) => {}
            Some(true) => {
                return Err(ApiError::new(
                    400,
                    "invalid_request_error",
                    "Streaming is not supported by this endpoint yet",
                )
                .with_param("stream")
                .with_code("unsupported_parameter"));
            }
            None => {
                return Err(ApiError::new(
                    400,
                    "invalid_request_error",
                    "The stream field must be a boolean",
                )
                .with_param("stream")
                .with_code("invalid_type"));
            }
        }
    }

    let values = object
        .get("messages")
        .and_then(JsonValue::as_array)
        .filter(|values| !values.is_empty() && values.len() <= MAX_MESSAGES)
        .ok_or_else(|| {
            ApiError::new(
                400,
                "invalid_request_error",
                "Messages must be a non-empty array of at most 128 entries",
            )
            .with_param("messages")
            .with_code("invalid_messages")
        })?;

    let mut messages = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let param = format!("messages[{index}]");
        let message = value.as_object().ok_or_else(|| {
            ApiError::new(
                400,
                "invalid_request_error",
                "Each message must be an object",
            )
            .with_param(&param)
            .with_code("invalid_type")
        })?;

        for key in message.keys() {
            if !matches!(key.as_str(), "role" | "content") {
                return Err(ApiError::new(
                    400,
                    "invalid_request_error",
                    "Unsupported message field",
                )
                .with_param(format!("{param}.{key}"))
                .with_code("unsupported_parameter"));
            }
        }

        let role = match message.get("role").and_then(JsonValue::as_str) {
            Some("system") => ChatRole::System,
            Some("user") => ChatRole::User,
            Some("assistant") => ChatRole::Assistant,
            _ => {
                return Err(ApiError::new(
                    400,
                    "invalid_request_error",
                    "Message role must be system, user, or assistant",
                )
                .with_param(format!("{param}.role"))
                .with_code("invalid_role"));
            }
        };

        let content = message
            .get("content")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                ApiError::new(
                    400,
                    "invalid_request_error",
                    "Message content must be a string",
                )
                .with_param(format!("{param}.content"))
                .with_code("invalid_type")
            })?;

        messages.push(ChatMessage {
            role,
            content: content.to_owned(),
        });
    }

    if messages.last().map(|message| message.role) != Some(ChatRole::User) {
        return Err(ApiError::new(
            400,
            "invalid_request_error",
            "The final message must have the user role",
        )
        .with_param("messages")
        .with_code("invalid_messages"));
    }

    Ok(NormalizedChatRequest { model_id, messages })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_messages_and_preserves_history_in_prompt() {
        let request = normalize_request(
            br#"{"model":"oib/chatgpt","messages":[{"role":"system","content":"Be concise"},{"role":"user","content":"First question"},{"role":"assistant","content":"First answer"},{"role":"user","content":"Second question"}],"stream":false}"#,
        )
        .unwrap();

        assert_eq!(request.model_id, "oib/chatgpt");
        assert_eq!(request.messages.len(), 4);
        assert_eq!(
            request.compose_prompt(),
            "System instruction:\nBe concise\n\nUser:\nFirst question\n\nAssistant:\nFirst answer\n\nUser:\nSecond question\n\nRespond to the final user message."
        );
    }

    #[test]
    fn rejects_invalid_json_and_unknown_fields() {
        assert_eq!(
            normalize_request(b"{").unwrap_err().code.as_deref(),
            Some("invalid_json")
        );
        let error = normalize_request(
            br#"{"model":"oib/chatgpt","messages":[{"role":"user","content":"hi"}],"temperature":0.2}"#,
        )
        .unwrap_err();
        assert_eq!(error.code.as_deref(), Some("unsupported_parameter"));
        assert_eq!(error.param.as_deref(), Some("temperature"));
    }

    #[test]
    fn rejects_streaming_and_invalid_message_shapes() {
        let streaming = normalize_request(
            br#"{"model":"oib/chatgpt","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
        )
        .unwrap_err();
        assert_eq!(streaming.code.as_deref(), Some("unsupported_parameter"));

        let invalid_role = normalize_request(
            br#"{"model":"oib/chatgpt","messages":[{"role":"tool","content":"hi"}]}"#,
        )
        .unwrap_err();
        assert_eq!(invalid_role.code.as_deref(), Some("invalid_role"));

        let invalid_content = normalize_request(
            br#"{"model":"oib/chatgpt","messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#,
        )
        .unwrap_err();
        assert_eq!(invalid_content.code.as_deref(), Some("invalid_type"));
    }

    #[test]
    fn requires_final_user_message_and_valid_model() {
        let final_assistant = normalize_request(
            br#"{"model":"oib/chatgpt","messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"hello"}]}"#,
        )
        .unwrap_err();
        assert_eq!(final_assistant.code.as_deref(), Some("invalid_messages"));

        let invalid_model =
            normalize_request(br#"{"model":"","messages":[{"role":"user","content":"hi"}]}"#)
                .unwrap_err();
        assert_eq!(invalid_model.param.as_deref(), Some("model"));
    }
}
