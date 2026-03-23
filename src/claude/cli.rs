use crate::provider::{default_model, default_models, ProviderModel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MODEL_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ModelCache {
    updated_at_unix_secs: u64,
    models: Vec<ProviderModel>,
}

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
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
            "--dangerously-skip-permissions".to_string(),
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

    pub fn discover_available_models(&self, overrides: &[String]) -> Vec<ProviderModel> {
        self.discover_available_models_with_cache_path(overrides, default_cache_path())
    }

    fn discover_available_models_with_cache_path(
        &self,
        overrides: &[String],
        cache_path: Option<PathBuf>,
    ) -> Vec<ProviderModel> {
        let overrides = override_models(overrides);
        if !overrides.is_empty() {
            return overrides;
        }

        if let Some(models) = cache_path
            .as_ref()
            .and_then(|path| self.read_cached_models(path))
        {
            return models;
        }

        let discovered = default_models()
            .into_iter()
            .filter_map(|model| self.probe_model(model))
            .collect::<Vec<_>>();

        if !discovered.is_empty() {
            if let Some(path) = cache_path.as_ref() {
                self.write_cached_models(path, &discovered);
            }
            return discovered;
        }

        default_models()
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    fn discovery_args(&self, model: &str) -> Vec<String> {
        vec![
            "--print".to_string(),
            "--model".to_string(),
            model.to_string(),
            "--output-format".to_string(),
            "json".to_string(),
            "Reply with ok".to_string(),
        ]
    }

    fn probe_model(&self, model: ProviderModel) -> Option<ProviderModel> {
        let output = Command::new(self.binary())
            .args(self.discovery_args(&model.id))
            .output()
            .ok()?;

        let value: Value = serde_json::from_slice(&output.stdout).ok()?;
        if value.get("is_error").and_then(Value::as_bool) != Some(false) {
            return None;
        }

        let display_name = value
            .get("modelUsage")
            .and_then(Value::as_object)
            .and_then(|usage| usage.keys().next())
            .map(|name| humanize_model_name(name))
            .unwrap_or(model.display_name);

        Some(ProviderModel::claude(model.id, display_name))
    }

    fn read_cached_models(&self, path: &Path) -> Option<Vec<ProviderModel>> {
        let cache: ModelCache = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        let age = now_unix_secs().saturating_sub(cache.updated_at_unix_secs);
        if age > MODEL_CACHE_TTL.as_secs() || cache.models.is_empty() {
            return None;
        }
        Some(cache.models)
    }

    fn write_cached_models(&self, path: &Path, models: &[ProviderModel]) {
        let Some(parent) = path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        let cache = ModelCache {
            updated_at_unix_secs: now_unix_secs(),
            models: models.to_vec(),
        };
        let Ok(bytes) = serde_json::to_vec_pretty(&cache) else {
            return;
        };
        let _ = fs::write(path, bytes);
    }
}

fn default_cache_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OPENCLAUDE_MODEL_CACHE") {
        return Some(PathBuf::from(path));
    }

    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|dir| dir.join("openclaude").join("models.json"))
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn override_models(overrides: &[String]) -> Vec<ProviderModel> {
    let mut models = Vec::new();
    for id in overrides
        .iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if models.iter().any(|model: &ProviderModel| model.id == id) {
            continue;
        }
        let display_name = default_model(id)
            .map(|model| model.display_name)
            .unwrap_or_else(|| display_name_for_override(id));
        models.push(ProviderModel::claude(id.to_string(), display_name));
    }
    models
}

fn display_name_for_override(id: &str) -> String {
    if id.starts_with("claude-") {
        return humanize_model_name(id);
    }

    let mut words = Vec::new();
    for part in id.split(['-', '_']) {
        if part.is_empty() {
            continue;
        }
        let mut chars = part.chars();
        let Some(first) = chars.next() else {
            continue;
        };
        let mut word = first.to_uppercase().collect::<String>();
        word.push_str(chars.as_str());
        words.push(word);
    }

    if words.is_empty() {
        id.to_string()
    } else {
        format!("Claude {}", words.join(" "))
    }
}

fn humanize_model_name(model_name: &str) -> String {
    let trimmed = model_name.strip_prefix("claude-").unwrap_or(model_name);
    let mut parts = trimmed.split('-');
    let family = parts.next().unwrap_or(trimmed);
    let family = match family {
        "haiku" => "Haiku",
        "sonnet" => "Sonnet",
        "opus" => "Opus",
        other => return other.to_string(),
    };

    let mut version = Vec::new();
    for part in parts {
        if part.len() == 8 && part.chars().all(|ch| ch.is_ascii_digit()) {
            break;
        }
        if part.chars().all(|ch| ch.is_ascii_digit()) {
            version.push(part);
        }
    }

    if version.is_empty() {
        format!("Claude {family}")
    } else {
        format!("Claude {family} {}", version.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn stream_args_include_prompt_when_present() {
        let cli = ClaudeCli::new("claude");
        let args = cli.stream_args("sonnet", Some("system"));
        assert!(args.contains(&"--permission-mode".to_string()));
        assert!(args.contains(&"bypassPermissions".to_string()));
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"--system-prompt".to_string()));
        assert!(args.contains(&"system".to_string()));
    }

    #[test]
    fn humanizes_versioned_model_name() {
        assert_eq!(
            humanize_model_name("claude-sonnet-4-6"),
            "Claude Sonnet 4.6"
        );
        assert_eq!(
            humanize_model_name("claude-haiku-4-5-20251001"),
            "Claude Haiku 4.5"
        );
    }

    #[test]
    fn override_models_win_over_probe_and_cache() {
        let cli = ClaudeCli::new("claude");
        let models = cli.discover_available_models_with_cache_path(
            &["sonnet,claude-sonnet-4-6".into(), "opus".into()],
            None,
        );
        let ids = models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>();
        let names = models
            .iter()
            .map(|model| model.display_name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["sonnet", "claude-sonnet-4-6", "opus"]);
        assert_eq!(
            names,
            vec!["Claude Sonnet", "Claude Sonnet 4.6", "Claude Opus"]
        );
    }

    #[test]
    fn discover_available_models_uses_fresh_cache() {
        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("models.json");
        fs::write(
            &cache_path,
            serde_json::to_vec(&ModelCache {
                updated_at_unix_secs: now_unix_secs(),
                models: vec![ProviderModel::claude("sonnet", "Claude Sonnet 4.6")],
            })
            .unwrap(),
        )
        .unwrap();

        let cli = ClaudeCli::new(dir.path().join("missing-claude"));
        let models = cli.discover_available_models_with_cache_path(&[], Some(cache_path));

        assert_eq!(
            models,
            vec![ProviderModel::claude("sonnet", "Claude Sonnet 4.6")]
        );
    }

    #[test]
    fn discover_available_models_prefers_cache_over_live_probe() {
        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("models.json");
        fs::write(
            &cache_path,
            serde_json::to_vec(&ModelCache {
                updated_at_unix_secs: now_unix_secs(),
                models: vec![ProviderModel::claude("sonnet", "Cached Sonnet")],
            })
            .unwrap(),
        )
        .unwrap();

        let script = dir.path().join("fake-claude.sh");
        fs::write(
            &script,
            "#!/usr/bin/env bash\nset -euo pipefail\nargs=\"$*\"\nif [[ \"$args\" == *\"--model haiku\"* ]]; then\n  printf '%s\\n' '{\"type\":\"result\",\"is_error\":false,\"modelUsage\":{\"claude-haiku-4-5-20251001\":{}}}'\nelse\n  printf '%s\\n' '{\"type\":\"result\",\"is_error\":true,\"modelUsage\":{}}'\nfi\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }

        let cli = ClaudeCli::new(&script);
        let models = cli.discover_available_models_with_cache_path(&[], Some(cache_path));

        assert_eq!(
            models,
            vec![ProviderModel::claude("sonnet", "Cached Sonnet")]
        );
    }

    #[test]
    fn discover_available_models_probes_when_cache_is_missing() {
        let dir = tempdir().unwrap();
        let script = dir.path().join("fake-claude.sh");
        fs::write(
            &script,
            "#!/usr/bin/env bash\nset -euo pipefail\nargs=\"$*\"\nif [[ \"$args\" == *\"--model sonnet\"* ]]; then\n  printf '%s\\n' '{\"type\":\"result\",\"is_error\":false,\"modelUsage\":{\"claude-sonnet-4-6\":{}}}'\nelse\n  printf '%s\\n' '{\"type\":\"result\",\"is_error\":true,\"modelUsage\":{}}'\nfi\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }

        let cli = ClaudeCli::new(&script);
        let models = cli.discover_available_models_with_cache_path(&[], None);

        assert_eq!(
            models,
            vec![ProviderModel::claude("sonnet", "Claude Sonnet 4.6")]
        );
    }

    #[test]
    fn discover_available_models_falls_back_when_cache_is_missing_and_probe_fails() {
        let dir = tempdir().unwrap();
        let cli = ClaudeCli::new(dir.path().join("missing-claude"));
        let models = cli.discover_available_models_with_cache_path(&[], None);

        assert_eq!(
            models.into_iter().map(|model| model.id).collect::<Vec<_>>(),
            vec!["haiku", "sonnet", "opus"]
        );
    }

    #[test]
    fn discover_available_models_filters_to_successful_aliases() {
        let dir = tempdir().unwrap();
        let script = dir.path().join("fake-claude.sh");
        fs::write(
            &script,
            r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
if [[ "$args" == *"--model haiku"* ]]; then
  printf '%s\n' '{"type":"result","is_error":false,"modelUsage":{"claude-haiku-4-5-20251001":{}}}'
elif [[ "$args" == *"--model opus"* ]]; then
  printf '%s\n' '{"type":"result","is_error":true,"modelUsage":{}}'
else
  printf '%s\n' '{"type":"result","is_error":false,"modelUsage":{"claude-sonnet-4-6":{}}}'
fi
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }

        let cli = ClaudeCli::new(&script);
        let models = cli.discover_available_models_with_cache_path(&[], None);
        let ids = models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>();
        let names = models
            .iter()
            .map(|model| model.display_name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["haiku", "sonnet"]);
        assert_eq!(names, vec!["Claude Haiku 4.5", "Claude Sonnet 4.6"]);
    }
}
