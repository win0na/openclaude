use crate::integration::{
    AdapterEvent, AdapterSessionState, AdapterStep, BridgeMessage, BridgeMessagePart,
    BridgeRequest, BridgeRole, OpenCodeBridge,
};
use crate::provider::ProviderRuntime;
use crate::server::openai::{
    ChatChoice, ChatChunk, ChatChunkChoice, ChatContent, ChatDelta, ChatFunctionCall,
    ChatFunctionCallDelta, ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatToolCall,
    ChatToolCallDelta, ChatToolType, ChatUsage,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::{get, post},
};
use futures::Stream;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};
use uuid::Uuid;

pub struct HttpState<R: ProviderRuntime + Clone + Send + Sync + 'static> {
    bridge: RwLock<OpenCodeBridge<R>>,
}

pub fn create_router<R: ProviderRuntime + Clone + Send + Sync + 'static>(
    bridge: OpenCodeBridge<R>,
) -> Router {
    let state = Arc::new(HttpState {
        bridge: RwLock::new(bridge),
    });

    Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/health", get(health))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn list_models<R: ProviderRuntime + Clone + Send + Sync + 'static>(
    State(state): State<Arc<HttpState<R>>>,
) -> impl IntoResponse {
    let bridge = state.bridge.read().await;
    let provider = bridge.provider_info();
    let models = bridge.models();

    Json(serde_json::json!({
        "object": "list",
        "data": models.iter().map(|m| {
            serde_json::json!({
                "id": m.id,
                "object": "model",
                "created": 0u64,
                "owned_by": provider.id,
            })
        }).collect::<Vec<_>>()
    }))
}

async fn chat_completions<R: ProviderRuntime + Clone + Send + Sync + 'static>(
    State(state): State<Arc<HttpState<R>>>,
    Json(request): Json<ChatRequest>,
) -> Response {
    info!(model = %request.model, stream = request.stream, "received chat completion request");

    let bridge_request = match to_bridge_request(&request) {
        Ok(req) => req,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "message": err,
                        "type": "invalid_request_error"
                    }
                })),
            )
                .into_response();
        }
    };

    if request.stream {
        let bridge = state.bridge.read().await;
        let events = match bridge.stream_events(bridge_request) {
            Ok(events) => events,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {
                            "message": err.to_string(),
                            "type": "internal_error"
                        }
                    })),
                )
                    .into_response();
            }
        };
        drop(bridge);

        let stream = stream_chat_response(request.model, events);
        Sse::new(stream).into_response()
    } else {
        let mut bridge = state.bridge.write().await;
        let step = match bridge.start(bridge_request) {
            Ok(step) => step,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {
                            "message": err.to_string(),
                            "type": "internal_error"
                        }
                    })),
                )
                    .into_response();
            }
        };

        let response = build_chat_response(request.model, step);
        Json(response).into_response()
    }
}

fn to_bridge_request(request: &ChatRequest) -> Result<BridgeRequest, String> {
    let system_prompt = request
        .messages
        .iter()
        .find(|m| m.role == ChatRole::System)
        .and_then(|m| m.content.as_text().map(|s| s.to_string()));

    let messages: Vec<BridgeMessage> = request
        .messages
        .iter()
        .filter(|m| m.role != ChatRole::System)
        .map(|m| {
            let role = match m.role {
                ChatRole::System => BridgeRole::System,
                ChatRole::User => BridgeRole::User,
                ChatRole::Assistant => BridgeRole::Assistant,
                ChatRole::Tool => BridgeRole::Tool,
            };

            let parts = match &m.content {
                ChatContent::Null => Vec::new(),
                ChatContent::Text(text) => vec![BridgeMessagePart::Text { text: text.clone() }],
                ChatContent::Parts(parts) => parts
                    .iter()
                    .filter_map(|p| match p {
                        crate::server::openai::ChatContentPart::Text { text } => {
                            Some(BridgeMessagePart::Text { text: text.clone() })
                        }
                        crate::server::openai::ChatContentPart::ImageUrl { .. } => None,
                    })
                    .collect(),
            };

            let mut parts = parts;
            if let Some(tool_calls) = &m.tool_calls {
                parts.extend(tool_calls.iter().map(|call| BridgeMessagePart::ToolCall {
                    call_id: call.id.clone(),
                    tool_name: call.function.name.clone(),
                    input: serde_json::from_str(&call.function.arguments)
                        .unwrap_or_else(|_| serde_json::json!({ "arguments": call.function.arguments })),
                }));
            }

            BridgeMessage { role, parts }
        })
        .collect();

    Ok(BridgeRequest {
        model_id: request.model.clone(),
        system_prompt: if system_prompt.is_some() {
            system_prompt
        } else {
            None
        },
        messages,
    })
}

fn build_chat_response(model: String, step: AdapterStep) -> ChatResponse {
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let (content, tool_calls, finish_reason) = extract_response_parts(step);

    ChatResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion".into(),
        created,
        model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: ChatRole::Assistant,
                content: if content.is_empty() && !tool_calls.is_empty() {
                    ChatContent::Null
                } else {
                    ChatContent::Text(content)
                },
                name: None,
                tool_call_id: None,
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
            },
            finish_reason,
        }],
        usage: Some(ChatUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        }),
    }
}

fn extract_response_parts(step: AdapterStep) -> (String, Vec<ChatToolCall>, Option<String>) {
    let mut content = String::new();
    let mut tool_calls: Vec<ChatToolCall> = Vec::new();
    let mut tool_call_buffers: HashMap<String, (String, String)> = HashMap::new();
    let mut finish_reason = None;

    for event in step.events {
        match event {
            AdapterEvent::TextDelta { delta, .. } => {
                content.push_str(&delta);
            }
            AdapterEvent::ToolInputStart { id, tool_name } => {
                tool_call_buffers.insert(id, (tool_name, String::new()));
            }
            AdapterEvent::ToolInputDelta { id, delta } => {
                if let Some((_, args)) = tool_call_buffers.get_mut(&id) {
                    args.push_str(&delta);
                }
            }
            AdapterEvent::ToolInputEnd { id } => {
                if let Some((id_clone, (name, args))) = tool_call_buffers.remove_entry(&id) {
                    tool_calls.push(ChatToolCall {
                        id: id_clone,
                        call_type: ChatToolType::Function,
                        function: ChatFunctionCall {
                            name,
                            arguments: args,
                        },
                    });
                }
            }
            AdapterEvent::ToolCall(call) => {
                tool_calls.push(ChatToolCall {
                    id: call.call_id,
                    call_type: ChatToolType::Function,
                    function: ChatFunctionCall {
                        name: call.tool_name,
                        arguments: call.input.to_string(),
                    },
                });
            }
            AdapterEvent::Finish { reason } => {
                finish_reason = Some(map_finish_reason(&reason).to_string());
            }
            _ => {}
        }
    }

    if finish_reason.is_none() {
        match step.state {
            AdapterSessionState::WaitingForTool(_) => {
                finish_reason = Some("tool_calls".into());
            }
            AdapterSessionState::Finished => {
                finish_reason = Some("stop".into());
            }
            AdapterSessionState::Ready => {}
        }
    }

    (content, tool_calls, finish_reason)
}

fn stream_chat_response(
    model: String,
    events: Box<dyn Iterator<Item = anyhow::Result<AdapterEvent>> + Send>,
) -> impl Stream<Item = Result<Event, axum::Error>> {
    let id = format!("chatcmpl-{}", Uuid::new_v4());
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let (tx, rx) = mpsc::channel(64);
    std::thread::spawn(move || {
        let mut state = StreamResponseState::new(id, created, model);
        let _ = tx.blocking_send(Ok(Event::default().data(make_role_chunk(
            &state.id,
            state.created,
            &state.model,
        ))));

        for event in events {
            match event {
                Ok(event) => {
                    for chunk in state.push(event) {
                        if tx.blocking_send(Ok(Event::default().data(chunk))).is_err() {
                            return;
                        }
                    }
                }
                Err(err) => {
                    error!(error = %err, "streaming response failed");
                    let _ = tx.blocking_send(Ok(Event::default().data(make_finish_chunk(
                        &state.id,
                        state.created,
                        &state.model,
                        "error",
                    ))));
                    let _ = tx.blocking_send(Err(axum::Error::new(err)));
                    return;
                }
            }
        }

        let _ = tx.blocking_send(Ok(Event::default().data("[DONE]")));
    });

    ReceiverStream::new(rx)
}

struct StreamResponseState {
    id: String,
    created: u64,
    model: String,
    tool_call_index: u32,
    saw_tool_call: bool,
    tool_call_buffers: HashMap<String, (u32, String, String)>,
}

impl StreamResponseState {
    fn new(id: String, created: u64, model: String) -> Self {
        Self {
            id,
            created,
            model,
            tool_call_index: 0,
            saw_tool_call: false,
            tool_call_buffers: HashMap::new(),
        }
    }

    fn push(&mut self, event: AdapterEvent) -> Vec<String> {
        match event {
            AdapterEvent::TextDelta { delta, .. } => {
                vec![make_content_chunk(&self.id, self.created, &self.model, &delta)]
            }
            AdapterEvent::ToolInputStart { id, tool_name } => {
                self.saw_tool_call = true;
                let idx = self.tool_call_index;
                self.tool_call_index += 1;
                self.tool_call_buffers
                    .insert(id.clone(), (idx, tool_name.clone(), String::new()));
                vec![make_tool_start_with_name_chunk(
                    &self.id,
                    self.created,
                    &self.model,
                    idx,
                    &id,
                    &tool_name,
                )]
            }
            AdapterEvent::ToolInputDelta { id, delta } => {
                if let Some((idx, _name, args)) = self.tool_call_buffers.get_mut(&id) {
                    args.push_str(&delta);
                    vec![make_tool_args_chunk(
                        &self.id,
                        self.created,
                        &self.model,
                        *idx,
                        &delta,
                    )]
                } else {
                    Vec::new()
                }
            }
            AdapterEvent::ToolInputEnd { id } => {
                if self.tool_call_buffers.remove(&id).is_some() {
                    Vec::new()
                } else {
                    Vec::new()
                }
            }
            AdapterEvent::ToolCall(_) => Vec::new(),
            AdapterEvent::Finish { reason } => {
                let mapped = if self.saw_tool_call && reason == "end_turn" {
                    "tool_call"
                } else {
                    &reason
                };
                vec![make_finish_chunk(&self.id, self.created, &self.model, mapped)]
            }
            _ => Vec::new(),
        }
    }
}

fn make_role_chunk(id: &str, created: u64, model: &str) -> String {
    let chunk = ChatChunk {
        id: id.into(),
        object: "chat.completion.chunk".into(),
        created,
        model: model.into(),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta: ChatDelta {
                role: Some(ChatRole::Assistant),
                content: None,
                tool_calls: None,
            },
            finish_reason: None,
        }],
    };
    serde_json::to_string(&chunk).unwrap()
}

fn make_content_chunk(id: &str, created: u64, model: &str, content: &str) -> String {
    let chunk = ChatChunk {
        id: id.into(),
        object: "chat.completion.chunk".into(),
        created,
        model: model.into(),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta: ChatDelta {
                role: None,
                content: Some(content.into()),
                tool_calls: None,
            },
            finish_reason: None,
        }],
    };
    serde_json::to_string(&chunk).unwrap()
}

fn make_tool_start_with_name_chunk(
    id: &str,
    created: u64,
    model: &str,
    index: u32,
    tool_id: &str,
    name: &str,
) -> String {
    make_tool_chunk(
        id,
        created,
        model,
        index,
        Some(tool_id.into()),
        Some(ChatToolType::Function),
        Some(name.into()),
        None,
    )
}

fn make_tool_chunk(
    id: &str,
    created: u64,
    model: &str,
    index: u32,
    tool_id: Option<String>,
    call_type: Option<ChatToolType>,
    name: Option<String>,
    arguments: Option<String>,
) -> String {
    let chunk = ChatChunk {
        id: id.into(),
        object: "chat.completion.chunk".into(),
        created,
        model: model.into(),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta: ChatDelta {
                role: None,
                content: None,
                tool_calls: Some(vec![ChatToolCallDelta {
                    index,
                    id: tool_id,
                    call_type,
                    function: match (name, arguments) {
                        (None, None) => None,
                        (name, arguments) => Some(ChatFunctionCallDelta { name, arguments }),
                    },
                }]),
            },
            finish_reason: None,
        }],
    };
    serde_json::to_string(&chunk).unwrap()
}

fn make_tool_args_chunk(id: &str, created: u64, model: &str, index: u32, args: &str) -> String {
    make_tool_chunk(
        id,
        created,
        model,
        index,
        None,
        Some(ChatToolType::Function),
        None,
        Some(args.into()),
    )
}

fn make_finish_chunk(id: &str, created: u64, model: &str, reason: &str) -> String {
    let chunk = ChatChunk {
        id: id.into(),
        object: "chat.completion.chunk".into(),
        created,
        model: model.into(),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta: ChatDelta {
                role: None,
                content: None,
                tool_calls: None,
            },
            finish_reason: Some(map_finish_reason(reason).into()),
        }],
    };
    serde_json::to_string(&chunk).unwrap()
}

fn map_finish_reason(reason: &str) -> &str {
    match reason {
        "end_turn" => "stop",
        "tool_call" => "tool_calls",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{
        ProviderInfo, ProviderModel, ProviderRequest, ProviderRuntime, StreamPart,
    };

    #[derive(Clone)]
    struct MockRuntime {
        model: ProviderModel,
    }

    impl ProviderRuntime for MockRuntime {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "mock".into(),
                name: "Mock".into(),
            }
        }

        fn models(&self) -> &[ProviderModel] {
            std::slice::from_ref(&self.model)
        }

        fn stream(
            &self,
            _request: ProviderRequest,
        ) -> anyhow::Result<Box<dyn Iterator<Item = anyhow::Result<StreamPart>> + Send>> {
            Ok(Box::new(vec![
                Ok(StreamPart::TextStart {
                    id: "part-0".into(),
                }),
                Ok(StreamPart::TextDelta(crate::provider::TextPart {
                    id: "part-0".into(),
                    delta: "hello".into(),
                })),
                Ok(StreamPart::TextEnd {
                    id: "part-0".into(),
                }),
                Ok(StreamPart::Finish {
                    reason: crate::provider::FinishReason::EndTurn,
                }),
            ]
            .into_iter()))
        }
    }

    #[test]
    fn builds_non_streaming_response() {
        let model = ProviderModel::claude("sonnet", "Claude Sonnet");
        let runtime = MockRuntime {
            model: model.clone(),
        };
        let mut bridge = OpenCodeBridge::new(runtime, vec![model]);

        let step = bridge
            .start(BridgeRequest {
                model_id: "sonnet".into(),
                system_prompt: None,
                messages: vec![BridgeMessage {
                    role: BridgeRole::User,
                    parts: vec![BridgeMessagePart::Text { text: "hi".into() }],
                }],
            })
            .unwrap();

        let response = build_chat_response("sonnet".into(), step);
        assert_eq!(response.model, "sonnet");
        assert_eq!(response.choices.len(), 1);
        assert!(
            matches!(response.choices[0].message.content, ChatContent::Text(ref s) if s == "hello")
        );
        assert_eq!(response.choices[0].finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn maps_internal_finish_reasons_to_openai_values() {
        assert_eq!(map_finish_reason("end_turn"), "stop");
        assert_eq!(map_finish_reason("tool_call"), "tool_calls");
        assert_eq!(map_finish_reason("error"), "error");
    }

    #[test]
    fn tool_start_chunk_includes_function_object_and_type() {
        let json = make_tool_start_with_name_chunk("chatcmpl-1", 1, "opus", 0, "toolu_1", "Read");
        let chunk: ChatChunk = serde_json::from_str(&json).unwrap();
        let tool_call = chunk.choices[0].delta.tool_calls.as_ref().unwrap()[0].clone();

        assert_eq!(tool_call.id.as_deref(), Some("toolu_1"));
        assert_eq!(tool_call.call_type, Some(ChatToolType::Function));
        assert_eq!(
            tool_call.function,
            Some(ChatFunctionCallDelta {
                name: Some("Read".into()),
                arguments: None,
            })
        );
    }

    #[test]
    fn tool_args_chunk_never_emits_invalid_function_name() {
        let json = make_tool_args_chunk("chatcmpl-1", 1, "opus", 0, r#"{"file_path":"/tmp/a"}"#);
        let chunk: ChatChunk = serde_json::from_str(&json).unwrap();
        let tool_call = chunk.choices[0].delta.tool_calls.as_ref().unwrap()[0].clone();

        assert_eq!(tool_call.function.unwrap().name, None);
    }

    #[test]
    fn bridge_request_preserves_assistant_tool_calls() {
        let request = ChatRequest {
            model: "sonnet".into(),
            messages: vec![ChatMessage {
                role: ChatRole::Assistant,
                content: ChatContent::Null,
                name: None,
                tool_call_id: None,
                tool_calls: Some(vec![ChatToolCall {
                    id: "toolu_1".into(),
                    call_type: ChatToolType::Function,
                    function: ChatFunctionCall {
                        name: "Read".into(),
                        arguments: r#"{"file_path":"/tmp/a"}"#.into(),
                    },
                }]),
            }],
            stream: false,
            temperature: None,
            top_p: None,
            max_tokens: None,
            tools: Vec::new(),
            tool_choice: None,
        };

        let bridge = to_bridge_request(&request).unwrap();
        assert!(matches!(
            &bridge.messages[0].parts[0],
            BridgeMessagePart::ToolCall { tool_name, .. } if tool_name == "Read"
        ));
    }

}
