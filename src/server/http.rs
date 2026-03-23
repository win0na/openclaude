use crate::integration::{
    AdapterEvent, AdapterSessionState, AdapterStep, BridgeMessage, BridgeMessagePart,
    BridgeRequest, BridgeRole, OpenCodeBridge,
};
use crate::provider::ProviderRuntime;
use crate::server::openai::{
    ChatChoice, ChatChunk, ChatChunkChoice, ChatContent, ChatDelta, ChatFunctionCall,
    ChatFunctionCallDelta, ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatToolCall,
    ChatToolCallDelta, ChatToolType, ChatUsage, format_sse, format_sse_done,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::{get, post},
};
use futures::Stream;
use futures::stream;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
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

    if request.stream {
        let stream = stream_chat_response(request.model, step);
        Sse::new(stream).into_response()
    } else {
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
                content: ChatContent::Text(content),
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
                finish_reason = Some(reason);
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
    step: AdapterStep,
) -> impl Stream<Item = Result<Event, axum::Error>> {
    let id = format!("chatcmpl-{}", Uuid::new_v4());
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let chunks = build_stream_chunks(id, created, model, step);
    stream::iter(chunks.into_iter().map(|s| Ok(Event::default().data(s))))
}

fn build_stream_chunks(id: String, created: u64, model: String, step: AdapterStep) -> Vec<String> {
    let mut chunks = Vec::new();

    chunks.push(make_role_chunk(&id, created, &model));

    let mut tool_call_index: u32 = 0;
    let mut tool_call_buffers: HashMap<String, (u32, String, String)> = HashMap::new();

    for event in step.events {
        match event {
            AdapterEvent::TextDelta { delta, .. } => {
                chunks.push(make_content_chunk(&id, created, &model, &delta));
            }
            AdapterEvent::ToolInputStart {
                id: tool_id,
                tool_name,
            } => {
                let idx = tool_call_index;
                tool_call_index += 1;
                tool_call_buffers.insert(tool_id.clone(), (idx, tool_name, String::new()));
                chunks.push(make_tool_start_chunk(&id, created, &model, idx, &tool_id));
            }
            AdapterEvent::ToolInputDelta { id: tool_id, delta } => {
                if let Some((idx, _name, args)) = tool_call_buffers.get_mut(&tool_id) {
                    args.push_str(&delta);
                    chunks.push(make_tool_args_chunk(&id, created, &model, *idx, &delta));
                }
            }
            AdapterEvent::ToolInputEnd { id: tool_id } => {
                if let Some((idx, name, _args)) = tool_call_buffers.remove(&tool_id) {
                    chunks.push(make_tool_name_chunk(&id, created, &model, idx, &name));
                }
            }
            AdapterEvent::ToolCall(call) => {
                let idx = tool_call_index;
                tool_call_index += 1;
                chunks.push(make_tool_start_chunk(
                    &id,
                    created,
                    &model,
                    idx,
                    &call.call_id,
                ));
                chunks.push(make_tool_name_chunk(
                    &id,
                    created,
                    &model,
                    idx,
                    &call.tool_name,
                ));
                chunks.push(make_tool_args_chunk(
                    &id,
                    created,
                    &model,
                    idx,
                    &call.input.to_string(),
                ));
            }
            AdapterEvent::Finish { reason } => {
                chunks.push(make_finish_chunk(&id, created, &model, &reason));
            }
            _ => {}
        }
    }

    chunks.push(format_sse_done());
    chunks
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
    format_sse(&serde_json::to_string(&chunk).unwrap())
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
    format_sse(&serde_json::to_string(&chunk).unwrap())
}

fn make_tool_start_chunk(id: &str, created: u64, model: &str, index: u32, tool_id: &str) -> String {
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
                    id: Some(tool_id.into()),
                    function: None,
                }]),
            },
            finish_reason: None,
        }],
    };
    format_sse(&serde_json::to_string(&chunk).unwrap())
}

fn make_tool_name_chunk(id: &str, created: u64, model: &str, index: u32, name: &str) -> String {
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
                    id: None,
                    function: Some(ChatFunctionCallDelta {
                        name: Some(name.into()),
                        arguments: None,
                    }),
                }]),
            },
            finish_reason: None,
        }],
    };
    format_sse(&serde_json::to_string(&chunk).unwrap())
}

fn make_tool_args_chunk(id: &str, created: u64, model: &str, index: u32, args: &str) -> String {
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
                    id: None,
                    function: Some(ChatFunctionCallDelta {
                        name: None,
                        arguments: Some(args.into()),
                    }),
                }]),
            },
            finish_reason: None,
        }],
    };
    format_sse(&serde_json::to_string(&chunk).unwrap())
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
            finish_reason: Some(reason.into()),
        }],
    };
    format_sse(&serde_json::to_string(&chunk).unwrap())
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
        ) -> anyhow::Result<std::vec::IntoIter<anyhow::Result<StreamPart>>> {
            Ok(vec![
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
            .into_iter())
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
    }
}
