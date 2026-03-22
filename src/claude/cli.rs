use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCli {
    pub binary: PathBuf,
}

impl ClaudeCli {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    pub fn stream_args(&self, model: &str, system_prompt: Option<&str>) -> Vec<String> {
        let mut args = vec![
            "--print".to_string(),
            "--model".to_string(),
            model.to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--include-partial-messages".to_string(),
        ];

        if let Some(prompt) = system_prompt.filter(|value| !value.is_empty()) {
            args.push("--system-prompt".to_string());
            args.push(prompt.to_string());
        }

        args
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_args_include_prompt_when_present() {
        let cli = ClaudeCli::new("claude");
        let args = cli.stream_args("sonnet", Some("system"));
        assert!(args.contains(&"--system-prompt".to_string()));
        assert!(args.contains(&"system".to_string()));
    }
}
