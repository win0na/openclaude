pub fn normalize_tool_name(name: &str) -> &str {
    match name {
        "Bash" => "bash",
        "Read" => "read",
        "Write" => "write",
        "Edit" => "edit",
        "MultiEdit" => "edit",
        "Glob" => "glob",
        "Grep" => "grep",
        "LS" | "List" => "glob",
        "WebFetch" => "webfetch",
        "WebSearch" => "websearch_web_search_exa",
        "ToolSearch" => "websearch_web_search_exa",
        "TodoWrite" => "todowrite",
        "Task" => "task",
        "Question" => "question",
        "Skill" => "skill",
        "NotebookEdit" => "edit",
        "NotebookRead" => "read",
        "ExitPlanMode" => "question",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_tool_name;

    #[test]
    fn normalizes_known() {
        let cases = [
            ("Bash", "bash"),
            ("Read", "read"),
            ("Write", "write"),
            ("Edit", "edit"),
            ("MultiEdit", "edit"),
            ("Glob", "glob"),
            ("Grep", "grep"),
            ("LS", "glob"),
            ("List", "glob"),
            ("WebFetch", "webfetch"),
            ("WebSearch", "websearch_web_search_exa"),
            ("ToolSearch", "websearch_web_search_exa"),
            ("TodoWrite", "todowrite"),
            ("Task", "task"),
            ("Question", "question"),
            ("Skill", "skill"),
            ("NotebookEdit", "edit"),
            ("NotebookRead", "read"),
            ("ExitPlanMode", "question"),
        ];

        for (input, expected) in cases {
            assert_eq!(normalize_tool_name(input), expected, "failed for {input}");
        }
    }

    #[test]
    fn leaves_unknown() {
        assert_eq!(normalize_tool_name("google_search"), "google_search");
        assert_eq!(normalize_tool_name("custom_tool"), "custom_tool");
    }
}
