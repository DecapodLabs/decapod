use crate::core::ansi::AnsiExt;
use crate::core::error::DecapodError;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const AUTH0_DOMAIN: &str = "decapod.auth0.com";
const AUTH0_CLIENT_ID: &str = "decapod-cli-client-id";
const AUTH0_AUDIENCE: &str = "https://api.decapodlabs.com";

#[derive(Deserialize, Debug)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

fn machine_data_dir() -> Result<PathBuf, DecapodError> {
    if let Ok(data_home) = env::var("XDG_DATA_HOME") {
        let trimmed = data_home.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed).join("decapod"));
        }
    }

    let home = env::var("HOME").map_err(|_| {
        DecapodError::ValidationError(
            "HOME is required to locate ~/.local/share/decapod for cloud credentials.".to_string(),
        )
    })?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("decapod"))
}

fn machine_session_token_path() -> Result<PathBuf, DecapodError> {
    Ok(machine_data_dir()?.join("session_token.json"))
}

pub fn perform_cloud_auth(_target_dir: &Path) -> Result<(), DecapodError> {
    // Check if curl is available
    if Command::new("curl").arg("--version").output().is_err() {
        return Err(DecapodError::ValidationError(
            "curl is required for cloud authentication. Please install curl and try again."
                .to_string(),
        ));
    }

    println!();
    println!("◢ {}", "Cloud Authentication".bright_cyan().bold());
    println!(
        "  {}",
        "Authenticating with Decapod Cloud. Credentials are stored outside this repository."
            .bright_black()
    );

    let device_code_res = initiate_device_flow()?;

    println!();
    println!(
        "  {}",
        "1. Open the following URL in your browser:"
            .bright_white()
            .bold()
    );
    println!(
        "     {}",
        device_code_res.verification_uri_complete.bright_blue()
    );
    println!();
    println!("  {}", "2. Ensure the code matches:".bright_white().bold());
    println!("     {}", device_code_res.user_code.bright_green().bold());
    println!();
    println!("  {}", "Waiting for authentication...".bright_black());

    let token = poll_for_token(&device_code_res)?;

    let data_dir = machine_data_dir()?;
    fs::create_dir_all(&data_dir).map_err(DecapodError::IoError)?;
    let token_path = machine_session_token_path()?;

    // Write token in JSON format
    let token_json = serde_json::json!({
        "token": token
    });
    let body = serde_json::to_string_pretty(&token_json)
        .map_err(|e| DecapodError::ValidationError(format!("Failed to serialize token: {e}")))?;
    fs::write(&token_path, body).map_err(DecapodError::IoError)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&token_path)
            .map_err(DecapodError::IoError)?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&token_path, perms).map_err(DecapodError::IoError)?;
    }

    println!(
        "{} {}",
        "✓".bright_green().bold(),
        "Cloud authentication successful. Session token saved.".bright_green()
    );

    Ok(())
}

pub fn is_token_valid(_target_dir: &Path) -> bool {
    let Ok(token_path) = machine_session_token_path() else {
        return false;
    };
    if !token_path.exists() {
        return false;
    }

    let token_str = match fs::read_to_string(&token_path) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let token = match serde_json::from_str::<serde_json::Value>(&token_str) {
        Ok(v) => v["token"]
            .as_str()
            .or_else(|| v["session_token"].as_str())
            .map(|s| s.to_string()),
        Err(_) => None,
    };

    let Some(token) = token else {
        return false;
    };

    if token.trim().is_empty() {
        return false;
    }

    // For now, we consider any non-empty token valid.
    // In the future, we could add a jwt decode/check here or a simple ping to an Auth0 endpoint.
    true
}

pub trait CloudAuthGate: Send + Sync {
    fn check_and_trigger(&self, root: &Path) -> Result<(), DecapodError>;
}

pub struct NoOpAuthGate;
impl CloudAuthGate for NoOpAuthGate {
    fn check_and_trigger(&self, _root: &Path) -> Result<(), DecapodError> {
        Ok(())
    }
}

pub struct InteractiveAuthGate;
impl CloudAuthGate for InteractiveAuthGate {
    fn check_and_trigger(&self, root: &Path) -> Result<(), DecapodError> {
        if !is_token_valid(root) {
            println!(
                "{} {}",
                "◢".bright_cyan().bold(),
                "Cloud authentication required".bright_white().bold()
            );
            perform_cloud_auth(root)?;
        }
        Ok(())
    }
}

pub fn get_cloud_auth_gate() -> Box<dyn CloudAuthGate> {
    use std::io::IsTerminal;
    // Trigger auth only if in a terminal and not in GITHUB_ACTIONS CI
    if std::io::stdin().is_terminal() && std::env::var("GITHUB_ACTIONS").is_err() {
        return Box::new(InteractiveAuthGate);
    }
    Box::new(NoOpAuthGate)
}

fn initiate_device_flow() -> Result<DeviceCodeResponse, DecapodError> {
    let url = format!("https://{AUTH0_DOMAIN}/oauth/device/code");
    let body = format!(
        "client_id={AUTH0_CLIENT_ID}&audience={AUTH0_AUDIENCE}&scope=openid profile email offline_access"
    );

    let output = Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            &url,
            "-H",
            "content-type: application/x-www-form-urlencoded",
            "-d",
            &body,
        ])
        .output()
        .map_err(|e| DecapodError::ValidationError(format!("Failed to execute curl: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(DecapodError::ValidationError(format!(
            "Auth0 device code request failed: {err}"
        )));
    }

    serde_json::from_slice(&output.stdout).map_err(|e| {
        DecapodError::ValidationError(format!(
            "Failed to parse Auth0 response: {e}. Raw: {}",
            String::from_utf8_lossy(&output.stdout)
        ))
    })
}

fn poll_for_token(device_code_res: &DeviceCodeResponse) -> Result<String, DecapodError> {
    let url = format!("https://{AUTH0_DOMAIN}/oauth/token");
    let body = format!(
        "grant_type=urn:ietf:params:oauth:grant-type:device_code&device_code={}&client_id={}",
        device_code_res.device_code, AUTH0_CLIENT_ID
    );

    let start = Instant::now();
    let expires_in = Duration::from_secs(device_code_res.expires_in);
    let interval = Duration::from_secs(if device_code_res.interval == 0 {
        5
    } else {
        device_code_res.interval
    });

    while start.elapsed() < expires_in {
        std::thread::sleep(interval);

        let output = Command::new("curl")
            .args([
                "-s",
                "-X",
                "POST",
                &url,
                "-H",
                "content-type: application/x-www-form-urlencoded",
                "-d",
                &body,
            ])
            .output()
            .map_err(|e| DecapodError::ValidationError(format!("Failed to execute curl: {e}")))?;

        if output.status.success() {
            let res: TokenResponse = serde_json::from_slice(&output.stdout).map_err(|e| {
                DecapodError::ValidationError(format!("Failed to parse Auth0 token response: {e}"))
            })?;

            if let Some(token) = res.access_token {
                return Ok(token);
            }

            if matches!(res.error.as_deref(), Some(err) if err != "authorization_pending") {
                return Err(DecapodError::ValidationError(format!(
                    "Auth0 error: {:?}",
                    res.error
                )));
            }
        }
    }

    Err(DecapodError::ValidationError(
        "Authentication timed out.".to_string(),
    ))
}
