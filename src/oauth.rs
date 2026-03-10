use base64::{engine::general_purpose, Engine as _};
use dialoguer::{theme::ColorfulTheme, Password};
use rand::{Rng, RngCore};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
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
    pub project_id: Option<String>,
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

    pub async fn refresh_token_if_needed(provider: &str) -> Self {
        let mut auth = Self::load();
        
        let token = match provider {
            "openai" | "openai_oauth" => auth.openai.as_ref(),
            "gemini" | "gemini_oauth" | "gemini_flash" => auth.gemini.as_ref(),
            "anthropic" | "anthropic_sonnet" => auth.anthropic.as_ref(),
            _ => return auth,
        };
        
        if let Some(t) = token {
            // 만료 5분 전이면 갱신
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            let expires_at = t.expires_at.unwrap_or(0);
            
            if now < expires_at - 300 {
                // 아직 유효 (5분 여유)
                return auth;
            }
            
            // refresh_token이 있으면 갱신 시도
            if let Some(ref refresh) = t.refresh_token {
                eprintln!("[AUTH] {} 토큰 만료됨, 자동 갱신 중...", provider);
                
                let result = match provider {
                    "openai" | "openai_oauth" => {
                        refresh_openai_token(refresh).await
                    }
                    "gemini" | "gemini_oauth" | "gemini_flash" => {
                        refresh_gemini_token(refresh).await
                    }
                    _ => None,
                };
                
                if let Some(new_token) = result {
                    match provider {
                        "openai" | "openai_oauth" => auth.openai = Some(new_token),
                        "gemini" | "gemini_oauth" | "gemini_flash" => auth.gemini = Some(new_token),
                        _ => {}
                    }
                    auth.save();
                    eprintln!("[AUTH] {} 토큰 갱신 완료!", provider);
                } else {
                    eprintln!("[AUTH] {} 토큰 갱신 실패. forja login {} 을 실행하세요.", provider, provider);
                }
            }
        }
        
        auth
    }
}

pub async fn run_login(provider: &str) {
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
    println!("Waiting for callback on http://localhost:1455/auth/callback ...");
    let listener = TcpListener::bind("127.0.0.1:1455").await.expect("Failed to bind port 1455");
    
    let result = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0; 4096];
                if let Ok(n) = socket.read(&mut buf).await {
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let lines: Vec<&str> = request.lines().collect();
                    if !lines.is_empty() {
                        let first_line = lines[0];
                        if first_line.starts_with("GET /auth/callback")
                            && let Some(query) = first_line.split_whitespace().nth(1) {
                                let parsed_url = url::Url::parse(&format!("http://localhost{}", query)).unwrap_or_else(|_| url::Url::parse("http://localhost").unwrap());
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
                    // /auth/callback이 아닌 요청은 404 처리하고 계속 대기
                    let response = "HTTP/1.1 404 Not Found\r\n\r\n";
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        }
    }).await;

    match result {
        Ok(code_opt) => code_opt,
        Err(_) => {
            println!("콜백 대기 시간(120초)이 초과되었습니다.");
            None
        }
    }
}

async fn exchange_code(
    token_url: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
    client_secret: Option<&str>,
) -> Result<serde_json::Value, String> {
    let client = Client::new();
    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("client_id", client_id),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret));
    }

    let resp = client
        .post(token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token 요청 실패: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("응답 읽기 실패: {e}"))?;

    if !status.is_success() {
        return Err(format!("Token 교환 실패 ({}): {}", status, body));
    }

    serde_json::from_str(&body)
        .map_err(|e| format!("JSON 파싱 실패: {e}"))
}

async fn login_openai() {
    let client_id = "app_EMoamEEZ73f0CkXaXp7hrann"; // In a real app, this should be a valid client ID
    let redirect_uri = "http://localhost:1455/auth/callback";
    let (verifier, challenge) = generate_pkce_challenge();
    let state: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    let auth_url = format!(
        "https://auth.openai.com/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope=openid profile email offline_access&state={}&codex_cli_simplified_flow=true&originator=codex_cli_rs",
        client_id, redirect_uri, challenge, state
    );

    println!("Opening browser for OpenAI login...");
    if open::that(&auth_url).is_err() {
        println!("브라우저를 열지 못했습니다. 아래 URL로 직접 접속해주세요:\n{}", auth_url);
    }

    if let Some(code) = wait_for_callback().await {
        println!("Received code. Exchanging for token...");

        match exchange_code(
            "https://auth.openai.com/oauth/token",
            client_id,
            &code,
            redirect_uri,
            &verifier,
            None,
        ).await {
            Ok(token_json) => {
                let access_token = token_json["access_token"].as_str().unwrap_or_default().to_string();
                let refresh_token = token_json["refresh_token"].as_str().map(|s| s.to_string());
                let expires_in = token_json["expires_in"].as_u64().unwrap_or(3600);
                let expires_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64 + expires_in as i64;
                
                let token = ProviderToken {
                    access_token,
                    refresh_token,
                    expires_at: Some(expires_at),
                    project_id: None,
                };
                
                let mut auth = AuthData::load();
                auth.openai = Some(token);
                auth.save();
                println!("OpenAI 로그인 및 토큰 저장이 완료되었습니다.");
            }
            Err(e) => {
                println!("OpenAI 토큰 교환 에러: {}", e);
            }
        }
    } else {
        println!("로그인에 실패했습니다. (콜백 없음)");
    }
}

async fn login_gemini() {
    let client_id = std::env::var("FORJA_GOOGLE_CLIENT_ID")
        .unwrap_or_else(|_| "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com".into());
    let client_id = client_id.as_str();
    let redirect_uri = "http://localhost:1455/auth/callback";
    let (verifier, challenge) = generate_pkce_challenge();

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope=openid%20https://www.googleapis.com/auth/cloud-platform%20https://www.googleapis.com/auth/userinfo.email%20https://www.googleapis.com/auth/userinfo.profile&access_type=offline&prompt=consent",
        client_id, redirect_uri, challenge
    );

    println!("Opening browser for Gemini login...");
    if open::that(&auth_url).is_err() {
        println!("브라우저를 열지 못했습니다. 아래 URL로 직접 접속해주세요:\n{}", auth_url);
    }

    if let Some(code) = wait_for_callback().await {
        println!("Received code. Exchanging for token...");
        
        let google_secret = std::env::var("FORJA_GOOGLE_CLIENT_SECRET")
            .unwrap_or_else(|_| "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".into());

        match exchange_code(
            "https://oauth2.googleapis.com/token",
            client_id,
            &code,
            redirect_uri,
            &verifier,
            Some(google_secret.as_str()),
        ).await {
            Ok(token_json) => {
                let access_token = token_json["access_token"].as_str().unwrap_or_default().to_string();
                let refresh_token = token_json["refresh_token"].as_str().map(|s| s.to_string());
                let expires_in = token_json["expires_in"].as_u64().unwrap_or(3600);
                let expires_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64 + expires_in as i64;
                
                // 동적으로 cloudaicompanionProject 값 가져오기
                let mut project_id = None;
                let client = Client::new();
                let assist_req = client.post("https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist")
                    .header("Authorization", format!("Bearer {}", access_token))
                    .header("Content-Type", "application/json")
                    .header("user-agent", "GeminiCLI/v22.12.0 (windows; x86_64)")
                    .json(&serde_json::json!({
                        "metadata": {
                            "ideType": "ANTIGRAVITY",
                            "pluginType": "GEMINI"
                        }
                    }))
                    .send()
                    .await;

                match assist_req {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_success() {
                            let text = resp.text().await.unwrap_or_default();
                            // 에러 추적을 위한 출력
                            // println!("CodeAssist Response: {}", text);
                            
                            if let Ok(resp_json) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(proj) = resp_json.get("cloudaicompanionProject").and_then(|v| v.as_str()) {
                                    project_id = Some(proj.to_string());
                                } else {
                                    println!("⚠️ cloudaicompanionProject 필드가 응답에 없습니다. 응답: {}", text);
                                }
                            } else {
                                println!("⚠️ 응답을 JSON으로 파싱할 수 없습니다. 응답: {}", text);
                            }
                        } else {
                            let text = resp.text().await.unwrap_or_default();
                            println!("⚠️ loadCodeAssist 요청 실패! 상태 코드: {}, 응답: {}", status, text);
                        }
                    }
                    Err(e) => {
                        println!("⚠️ loadCodeAssist 요청 자체가 실패했습니다: {:?}", e);
                    }
                }

                let token = ProviderToken {
                    access_token,
                    refresh_token,
                    expires_at: Some(expires_at),
                    project_id,
                };
                
                let mut auth = AuthData::load();
                auth.gemini = Some(token);
                auth.save();
                println!("Gemini 로그인 및 토큰 저장이 완료되었습니다.");
            }
            Err(e) => {
                println!("Gemini 토큰 교환 에러: {}", e);
            }
        }
    } else {
        println!("로그인에 실패했습니다. (콜백 없음)");
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
        project_id: None,
    };

    let mut auth = AuthData::load();
    auth.anthropic = Some(token);
    auth.save();
    println!("Anthropic 토큰 저장이 완료되었습니다.");
}

async fn refresh_openai_token(refresh_token: &str) -> Option<ProviderToken> {
    let client = reqwest::Client::new();
    let resp = client.post("https://auth.openai.com/oauth/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
        ])
        .send()
        .await
        .ok()?;
    
    if !resp.status().is_success() { return None; }
    
    let json: serde_json::Value = resp.json().await.ok()?;
    let access_token = json["access_token"].as_str()?.to_string();
    let new_refresh = json["refresh_token"].as_str()
        .map(|s| s.to_string())
        .or_else(|| Some(refresh_token.to_string()));
    let expires_in = json["expires_in"].as_u64().unwrap_or(864000);
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64 + expires_in as i64;
    
    Some(ProviderToken {
        access_token,
        refresh_token: new_refresh,
        expires_at: Some(expires_at),
        project_id: None,
    })
}

async fn refresh_gemini_token(refresh_token: &str) -> Option<ProviderToken> {
    let client_id = std::env::var("FORJA_GOOGLE_CLIENT_ID")
        .unwrap_or_else(|_| "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com".into());
    let client_secret = std::env::var("FORJA_GOOGLE_CLIENT_SECRET")
        .unwrap_or_else(|_| "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".into());
    
    let client = reqwest::Client::new();
    let resp = client.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &client_id),
            ("client_secret", &client_secret),
        ])
        .send()
        .await
        .ok()?;
    
    if !resp.status().is_success() { return None; }
    
    let json: serde_json::Value = resp.json().await.ok()?;
    let access_token = json["access_token"].as_str()?.to_string();
    let new_refresh = json["refresh_token"].as_str()
        .map(|s| s.to_string())
        .or_else(|| Some(refresh_token.to_string()));
    let expires_in = json["expires_in"].as_u64().unwrap_or(3600);
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64 + expires_in as i64;
    
    // 기존 project_id 보존
    let auth = AuthData::load();
    let project_id = auth.gemini.and_then(|t| t.project_id);
    
    Some(ProviderToken {
        access_token,
        refresh_token: new_refresh,
        expires_at: Some(expires_at),
        project_id,
    })
}
