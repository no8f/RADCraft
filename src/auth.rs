use anyhow::{Result, anyhow};
use minecraft_msa_auth::MinecraftAuthorizationFlow;
use oauth2::basic::BasicClient;
use oauth2::devicecode::StandardDeviceAuthorizationResponse;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, ClientId, DeviceAuthorizationUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MSA_AUTHORIZE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const MSA_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const DEFAULT_CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrosoftAccount {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
}

#[derive(Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
}

impl MicrosoftAccount {
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Refresh if within 5 minutes of expiry
        now + 300 >= self.expires_at
    }

    pub async fn load(config_dir: &Path) -> Option<Self> {
        let path = config_dir.join("account.json");
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                if let Ok(account) = serde_json::from_str::<Self>(&content) {
                    return Some(account);
                }
            }
        }
        None
    }

    pub async fn save(&self, config_dir: &Path) -> Result<()> {
        let path = config_dir.join("account.json");
        let json = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&path, json).await?;
        Ok(())
    }

    pub async fn remove(config_dir: &Path) -> Result<()> {
        let path = config_dir.join("account.json");
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }
}

fn build_oauth_client() -> Result<BasicClient> {
    let client_id =
        std::env::var("RCRAFT_MSA_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());

    let client = BasicClient::new(
        ClientId::new(client_id),
        None,
        AuthUrl::new(MSA_AUTHORIZE_URL.to_string())?,
        Some(TokenUrl::new(MSA_TOKEN_URL.to_string())?),
    )
    .set_device_authorization_url(DeviceAuthorizationUrl::new(DEVICE_CODE_URL.to_string())?);

    Ok(client)
}

pub async fn start_device_code_flow() -> Result<StandardDeviceAuthorizationResponse> {
    let client = build_oauth_client()?;
    let details: StandardDeviceAuthorizationResponse = client
        .exchange_device_code()?
        .add_scope(Scope::new("XboxLive.signin".to_string()))
        .add_scope(Scope::new("offline_access".to_string()))
        .request_async(async_http_client)
        .await
        .map_err(|e| anyhow!("Failed to request device code: {}", e))?;

    Ok(details)
}

pub async fn complete_device_code_flow(
    details: StandardDeviceAuthorizationResponse,
) -> Result<MicrosoftAccount> {
    let client = build_oauth_client()?;
    let token = client
        .exchange_device_access_token(&details)
        .request_async(async_http_client, tokio::time::sleep, None)
        .await
        .map_err(|e| anyhow!("Microsoft authentication failed or timed out: {}", e))?;

    let ms_access_token = token.access_token().secret().to_string();
    let refresh_token = token.refresh_token().map(|r| r.secret().to_string());

    exchange_for_minecraft_account(&ms_access_token, refresh_token).await
}

pub async fn refresh_minecraft_account(account: &MicrosoftAccount) -> Result<MicrosoftAccount> {
    let refresh_secret = account
        .refresh_token
        .as_ref()
        .ok_or_else(|| anyhow!("No refresh token available; please sign in again."))?;

    let client = build_oauth_client()?;
    let token = client
        .exchange_refresh_token(&RefreshToken::new(refresh_secret.clone()))
        .request_async(async_http_client)
        .await
        .map_err(|e| anyhow!("Failed to refresh Microsoft token: {}", e))?;

    let ms_access_token = token.access_token().secret().to_string();
    let new_refresh_token = token
        .refresh_token()
        .map(|r| r.secret().to_string())
        .or_else(|| account.refresh_token.clone());

    exchange_for_minecraft_account(&ms_access_token, new_refresh_token).await
}

async fn exchange_for_minecraft_account(
    ms_access_token: &str,
    refresh_token: Option<String>,
) -> Result<MicrosoftAccount> {
    let http_client = reqwest::Client::new();
    let mc_flow = MinecraftAuthorizationFlow::new(http_client.clone());

    let mc_token = mc_flow
        .exchange_microsoft_token(ms_access_token)
        .await
        .map_err(|e| {
            anyhow!(
                "Failed to exchange Microsoft token for Minecraft token: {}",
                e
            )
        })?;

    let access_token = mc_token.access_token().as_ref().to_string();
    let expires_in = mc_token.expires_in() as u64;

    let resp = http_client
        .get(MC_PROFILE_URL)
        .bearer_auth(&access_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(anyhow!(
            "Could not fetch Minecraft profile (HTTP {}). Make sure this Microsoft account owns Minecraft: Java Edition.",
            resp.status()
        ));
    }

    let profile: McProfileResponse = resp.json().await?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(MicrosoftAccount {
        username: profile.name,
        uuid: profile.id,
        access_token,
        refresh_token,
        expires_at: now + expires_in,
    })
}

pub async fn fetch_avatar_icon(uuid: &str) -> Option<PathBuf> {
    let cache_dir = std::env::temp_dir()
        .join("radcraft")
        .join("cache")
        .join("avatars");
    let _ = tokio::fs::create_dir_all(&cache_dir).await;
    let icon_path = cache_dir.join(format!("{}.png", uuid));

    if icon_path.exists() {
        return Some(icon_path);
    }

    let url = format!("https://mc-heads.net/avatar/{}/32", uuid);
    if let Ok(resp) = reqwest::get(&url).await {
        if resp.status().is_success() {
            if let Ok(bytes) = resp.bytes().await {
                if tokio::fs::write(&icon_path, &bytes).await.is_ok() {
                    return Some(icon_path);
                }
            }
        }
    }

    None
}
