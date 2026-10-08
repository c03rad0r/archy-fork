//! Owner-selected chat provider. API keys remain in the node's private secret
//! ledger and are never returned by settings or included in chat context.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::{fs, io::AsyncWriteExt};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Auto,
    Claude,
    Openai,
    Local,
    Routstr,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelProvider {
    #[serde(default)]
    pub provider: Provider,
    #[serde(default)]
    pub openai_model: String,
}

impl ModelProvider {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            !self.openai_model.starts_with("sk-")
                && self.openai_model.len() <= 128
                && self
                    .openai_model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b)),
            "Invalid OpenAI model name"
        );
        anyhow::ensure!(
            self.provider != Provider::Openai || !self.openai_model.is_empty(),
            "Choose an OpenAI model before connecting"
        );
        Ok(())
    }
    pub async fn load(data_dir: &Path) -> Result<Self> {
        match fs::read(data_dir.join("settings/model-provider.json")).await {
            Ok(bytes) => {
                let settings: Self = serde_json::from_slice(&bytes)
                    .context("Invalid AI provider settings; preserved for recovery")?;
                settings.validate()?;
                Ok(settings)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }
    pub async fn save(&self, data_dir: &Path) -> Result<()> {
        self.validate()?;
        write_private(
            &data_dir.join("settings/model-provider.json"),
            &serde_json::to_vec(self)?,
        )
        .await
    }
}

pub fn key_name(provider: &str) -> Result<&'static str> {
    match provider {
        "claude" => Ok("claude-api-key"),
        "openai" => Ok("openai-api-key"),
        _ => anyhow::bail!("Unsupported AI provider"),
    }
}
pub async fn has_key(data_dir: &Path, provider: &str) -> bool {
    let Ok(name) = key_name(provider) else {
        return false;
    };
    fs::read_to_string(data_dir.join("secrets").join(name))
        .await
        .is_ok_and(|key| !key.trim().is_empty())
}
pub async fn save_key(data_dir: &Path, provider: &str, value: &str) -> Result<()> {
    let path = data_dir.join("secrets").join(key_name(provider)?);
    let value = value.trim();
    anyhow::ensure!(
        value.len() <= 4096 && value.bytes().all(|b| b.is_ascii_graphic()),
        "Invalid API key format"
    );
    if value.is_empty() {
        match fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    } else {
        write_private(&path, value.as_bytes()).await
    }
}
async fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Missing settings directory")?;
    fs::create_dir_all(parent).await?;
    let temporary = parent.join(format!(".provider-{}.tmp", uuid::Uuid::new_v4()));
    let result = async {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&temporary, path).await?;
        fs::File::open(parent).await?.sync_all().await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(temporary).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn private_keys_replace_atomically_and_never_enter_public_settings() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_key(dir.path(), "openai").await);
        save_key(dir.path(), "openai", "test-key-one")
            .await
            .unwrap();
        save_key(dir.path(), "openai", "test-key-two")
            .await
            .unwrap();
        assert!(has_key(dir.path(), "openai").await);
        let key_path = dir.path().join("secrets/openai-api-key");
        assert_eq!(
            fs::metadata(&key_path).await.unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_to_string(&key_path).await.unwrap(), "test-key-two");
        let settings = ModelProvider {
            provider: Provider::Openai,
            openai_model: "test-model".into(),
        };
        settings.save(dir.path()).await.unwrap();
        let body = serde_json::to_string(&ModelProvider::load(dir.path()).await.unwrap()).unwrap();
        assert!(!body.contains("test-key"));
        assert!(save_key(dir.path(), "../openai", "key").await.is_err());
        assert!(save_key(dir.path(), "openai", "key\nInjected: bad")
            .await
            .is_err());
        assert_eq!(fs::read_to_string(&key_path).await.unwrap(), "test-key-two");
        save_key(dir.path(), "openai", "").await.unwrap();
        assert!(!has_key(dir.path(), "openai").await);
    }
    #[tokio::test]
    async fn invalid_settings_preserve_existing_configuration() {
        let dir = tempfile::tempdir().unwrap();
        ModelProvider::default().save(dir.path()).await.unwrap();
        let invalid = ModelProvider {
            provider: Provider::Openai,
            openai_model: String::new(),
        };
        assert!(invalid.save(dir.path()).await.is_err());
        assert_eq!(
            ModelProvider::load(dir.path()).await.unwrap().provider,
            Provider::Auto
        );
        let path = dir.path().join("settings/model-provider.json");
        fs::write(&path, b"broken").await.unwrap();
        assert!(ModelProvider::load(dir.path()).await.is_err());
        assert_eq!(fs::read(&path).await.unwrap(), b"broken");
    }
}
