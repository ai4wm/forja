use base64::{engine::general_purpose, Engine as _};
use dialoguer::{theme::ColorfulTheme, Password};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AuthData {
    pub openai: Option<ProviderToken>,
    pub gemini: Option<ProviderToken>,
    pub anthropic: Option<ProviderToken>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

pub fn auth_file_path() -> PathBuf {
    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".forja").join("auth.json")
}

impl AuthData {
    pub fn load() -> Self {
        let path = auth_file_path();
        if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = auth_file_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, content);
        }
    }

    pub async fn load_and_refresh() -> Self {
        // Here we would implement refresh logic if expires_at < now.
        // For simplicity and immediate scope, we return loaded data.
        Self::load()
    }
}

pub async fn handle_login(provider: &str) {
    let provider = provider.to_lowercase();
    match provider.as_str() {
        "openai" => login_openai().await,
        "gemini" => login_gemini().await,
        "anthropic" => login_anthropic().await,
        _ => {
            println!("지원하지 않는 프로바이더입니다: {}", provider);
            println!("가능한 옵션: openai, gemini, anthropic");
        }
    }
}

fn generate_pkce_challenge() -> (String, String) {
    let mut verifier_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut verifier_bytes);
    let verifier = general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());

    (verifier, challenge)
}

async fn wait_for_callback() -> Option<String> {
    println!("Waiting for callback on http://127.0.0.1:1455/callback ...");
    let listener = TcpListener::bind("127.0.0.1:1455").await.expect("Failed to bind port 1455");
    
    if let Ok((mut socket, _)) = listener.accept().await {
        let mut buf = [0; 1024];
        if let Ok(n) = socket.read(&mut buf).await {
            let request = String::from_utf8_lossy(&buf[..n]);
            let lines: Vec<&str> = request.lines().collect();
            if !lines.is_empty() {
                let first_line = lines[0];
                if first_line.starts_with("GET /callback")
                    && let Some(query) = first_line.split_whitespace().nth(1) {
                        let parsed_url = url::Url::parse(&format!("http://localhost{}", query)).unwrap();
                        let mut code = None;
                        for (k, v) in parsed_url.query_pairs() {
                            if k == "code" {
                                code = Some(v.into_owned());
                            }
                        }
                        
                        let response = "HTTP/1.1 200 OK\r\n\r\n<html><body><h1>Login Successful!</h1><p>You can close this window now.</p></body></html>";
                        let _ = socket.write_all(response.as_bytes()).await;
                        return code;
                    }
            }
        }
    }
    None
}

async fn login_openai() {
    let client_id = "your-openai-client-id"; // In a real app, this should be a valid client ID
    let redirect_uri = "http://127.0.0.1:1455/callback";
    let (verifier, challenge) = generate_pkce_challenge();

    let auth_url = format!(
        "https://auth.openai.com/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope=offline_access",
        client_id, redirect_uri, challenge
    );

    println!("Opening browser for OpenAI login...");
    if open::that(&auth_url).is_err() {
        println!("브라우저를 열지 못했습니다. 아래 URL로 직접 접속해주세요:\n{}", auth_url);
    }

    if let Some(code) = wait_for_callback().await {
        println!("Received code. In a real app, we would exchange this for a token with the verifier: {}", verifier);
        // Note: For actual token exchange, you'd make a POST to the token endpoint.
        // Here we simulate successful token receipt.
        
        let token = ProviderToken {
            access_token: format!("mock-openai-token-from-code-{}", code),
            refresh_token: None,
            expires_at: None,
        };
        
        let mut auth = AuthData::load();
        auth.openai = Some(token);
        auth.save();
        println!("OpenAI 로그인 및 토큰 저장이 완료되었습니다.");
    } else {
        println!("로그인에 실패했습니다.");
    }
}

async fn login_gemini() {
    let client_id = "your-google-client-id"; // Placeholder
    let redirect_uri = "http://127.0.0.1:1455/callback";
    let (verifier, challenge) = generate_pkce_challenge();

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope=https://www.googleapis.com/auth/cloud-platform",
        client_id, redirect_uri, challenge
    );

    println!("Opening browser for Gemini login...");
    if open::that(&auth_url).is_err() {
        println!("브라우저를 열지 못했습니다. 아래 URL로 직접 접속해주세요:\n{}", auth_url);
    }

    if let Some(code) = wait_for_callback().await {
        println!("Received code. In a real app, we would exchange this for a token with the verifier: {}", verifier);
        let token = ProviderToken {
            access_token: format!("mock-gemini-token-from-code-{}", code),
            refresh_token: None,
            expires_at: None,
        };
        
        let mut auth = AuthData::load();
        auth.gemini = Some(token);
        auth.save();
        println!("Gemini 로그인 및 토큰 저장이 완료되었습니다.");
    } else {
        println!("로그인에 실패했습니다.");
    }
}

async fn login_anthropic() {
    println!("Anthropic은 OAuth를 지원하지 않습니다.");
    let token = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("Anthropic API 키를 붙여넣으세요")
        .interact()
        .unwrap();

    let token = ProviderToken {
        access_token: token,
        refresh_token: None,
        expires_at: None,
    };

    let mut auth = AuthData::load();
    auth.anthropic = Some(token);
    auth.save();
    println!("Anthropic 토큰 저장이 완료되었습니다.");
}
