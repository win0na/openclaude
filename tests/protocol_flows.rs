use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use openclaude::integration::{
    AdapterEvent, AdapterSessionState, BridgeMessage, BridgeMessagePart, BridgeRequest,
    BridgeRole, OpenCodeBridge,
};
use openclaude::provider::{
    FinishReason, ProviderInfo, ProviderModel, ProviderRequest, ProviderRuntime, StreamPart,
    ToolCallPart, ToolInputDeltaPart, ToolInputStartPart,
};
use openclaude::server::{
    ChatRequest, ChatResponse, OpenClaudeService, ServerCommand, ServerRequest, create_router,
    serve_stdio,
};
use tower::ServiceExt;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn parses_basic() {
    let request: ChatRequest = serde_json::from_str(&fixture("basic_chat.json")).unwrap();
    assert_eq!(request.model, "sonnet");
    assert!(!request.stream);
}

#[test]
fn parses_choice() {
    let request: ChatRequest =
        serde_json::from_str(&fixture("tool_choice_function.json")).unwrap();
    assert_eq!(request.model, "opus");
    assert!(request.stream);
    assert!(request.tool_choice.is_some());
    assert_eq!(request.tools.len(), 1);
}

#[test]
fn parses_history() {
    let request: ChatRequest =
        serde_json::from_str(&fixture("assistant_tool_history.json")).unwrap();
    assert_eq!(request.messages.len(), 3);
    assert!(request.messages[0].tool_calls.as_ref().is_some());
}

#[derive(Clone)]
struct TextRuntime {
    model: ProviderModel,
}

impl ProviderRuntime for TextRuntime {
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
        Ok(Box::new(
            vec![
                Ok(StreamPart::TextStart {
                    id: "part-0".into(),
                }),
                Ok(StreamPart::TextDelta(openclaude::provider::TextPart {
                    id: "part-0".into(),
                    delta: "hello".into(),
                })),
                Ok(StreamPart::TextEnd {
                    id: "part-0".into(),
                }),
                Ok(StreamPart::Finish {
                    reason: FinishReason::EndTurn,
                }),
            ]
            .into_iter(),
        ))
    }
}

#[derive(Clone)]
struct ToolRuntime {
    model: ProviderModel,
}

#[derive(Clone)]
struct ServiceToolRuntime {
    model: ProviderModel,
}

impl ProviderRuntime for ServiceToolRuntime {
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
        Ok(Box::new(
            vec![
                Ok(StreamPart::ToolCall(ToolCallPart {
                    id: "toolu_1".into(),
                    tool_call_id: "toolu_1".into(),
                    tool_name: "Read".into(),
                    input: serde_json::json!({"file_path": "/tmp/a"}),
                })),
                Ok(StreamPart::Finish {
                    reason: FinishReason::ToolCall,
                }),
            ]
            .into_iter(),
        ))
    }
}

impl ProviderRuntime for ToolRuntime {
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
        Ok(Box::new(
            vec![
                Ok(StreamPart::ToolInputStart(ToolInputStartPart {
                    id: "toolu_1".into(),
                    tool_name: "Read".into(),
                })),
                Ok(StreamPart::ToolInputDelta(ToolInputDeltaPart {
                    id: "toolu_1".into(),
                    delta: r#"{"file_path":"/tmp/a"}"#.into(),
                })),
                Ok(StreamPart::ToolInputEnd {
                    id: "toolu_1".into(),
                }),
                Ok(StreamPart::Finish {
                    reason: FinishReason::ToolCall,
                }),
            ]
            .into_iter(),
        ))
    }
}

#[tokio::test]
async fn http_completion() {
    let model = ProviderModel::claude("sonnet", "Claude Sonnet");
    let router = create_router(OpenCodeBridge::new(
        TextRuntime {
            model: model.clone(),
        },
        vec![model],
    ));

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "model": "sonnet",
                "messages": [{"role": "user", "content": "hello"}],
                "stream": false
            })
            .to_string(),
        ))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: ChatResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.choices[0].message.content, openclaude::server::ChatContent::Text("hello".into()));
    assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("stop"));
}

#[tokio::test]
async fn http_stream() {
    let model = ProviderModel::claude("sonnet", "Claude Sonnet");
    let router = create_router(OpenCodeBridge::new(
        ToolRuntime {
            model: model.clone(),
        },
        vec![model],
    ));

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "model": "sonnet",
                "messages": [{"role": "user", "content": "use a tool"}],
                "stream": true
            })
            .to_string(),
        ))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains(r#""type":"function""#));
    assert!(text.contains(r#""function":{"name":"Read"}"#));
    assert!(!text.contains(r#""name":null"#));
    assert!(text.contains(r#""finish_reason":"tool_calls""#));
    assert!(text.contains("[DONE]"));
}

#[test]
fn service_history() {
    let model = ProviderModel::claude("sonnet", "Claude Sonnet");
    let bridge = OpenCodeBridge::new(
        ServiceToolRuntime {
            model: model.clone(),
        },
        vec![model],
    );
    let mut service = OpenClaudeService::new(bridge);

    let first = service
        .complete(ServerRequest {
            conversation: BridgeRequest {
                model_id: "sonnet".into(),
                system_prompt: None,
                messages: vec![BridgeMessage {
                    role: BridgeRole::User,
                    parts: vec![BridgeMessagePart::Text {
                        text: "earlier\n\nhello".into(),
                    }],
                }],
            },
        })
        .unwrap();

    assert!(matches!(first.step.state, AdapterSessionState::WaitingForTool(_)));
    assert!(first
        .step
        .events
        .iter()
        .any(|event| matches!(event, AdapterEvent::ToolCall(_))));
}

#[test]
fn stdio_complete() {
    let model = ProviderModel::claude("sonnet", "Claude Sonnet");
    let bridge = OpenCodeBridge::new(
        ToolRuntime {
            model: model.clone(),
        },
        vec![model],
    );
    let mut service = OpenClaudeService::new(bridge);

    let input = [
        serde_json::to_string(&ServerCommand::Describe {
            request_id: "req-1".into(),
        })
        .unwrap(),
        serde_json::to_string(&ServerCommand::Complete {
            request_id: "req-2".into(),
            request: openclaude::server::ServerRequest {
                conversation: BridgeRequest {
                    model_id: "sonnet".into(),
                    system_prompt: None,
                    messages: vec![BridgeMessage {
                        role: BridgeRole::User,
                        parts: vec![BridgeMessagePart::Text {
                            text: "earlier\n\nhello".into(),
                        }],
                    }],
                },
            },
        })
        .unwrap(),
    ]
    .join("\n");

    let mut output = Vec::new();
    serve_stdio(&mut service, input.as_bytes(), &mut output).unwrap();
    let responses = String::from_utf8(output).unwrap();

    assert!(responses.contains("\"provider_id\":\"mock\""));
    assert!(responses.contains("\"kind\":\"success\""));
    assert!(responses.contains("\"request_id\":\"req-1\""));
    assert!(responses.contains("\"request_id\":\"req-2\""));
}
