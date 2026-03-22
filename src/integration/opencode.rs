pub const LOCAL_PROVIDER_FILE: &str =
    "/home/winnie/claude/opencode/packages/opencode/src/provider/provider.ts";
pub const LOCAL_SESSION_PROCESSOR_FILE: &str =
    "/home/winnie/claude/opencode/packages/opencode/src/session/processor.ts";

pub const INTEGRATION_TARGETS: &[&str] = &[
    "Provider.list / custom loader registration in provider.ts",
    "AI SDK stream part handling in session/processor.ts",
    "reasoning/tool/text part persistence in session/message-v2.ts",
];

pub fn summary() -> String {
    format!(
        "Targets: {} | {} | {}",
        INTEGRATION_TARGETS[0], INTEGRATION_TARGETS[1], INTEGRATION_TARGETS[2]
    )
}
