use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};

use crate::http::{HttpResponse, http_request, open_url, url_encode};
use crate::paths::{AppPaths, SecretFile};
use crate::strings::Language;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GoogleCredential {
    pub refresh_token: String,
    pub email: String,
    pub scopes: Option<Vec<String>>,
}

pub struct GoogleClient {
    pub id: String,
    pub secret: String,
}

impl GoogleClient {
    pub fn parse_json(raw: &str) -> Option<Self> {
        let doc: Value = serde_json::from_str(raw.trim_start_matches('\u{FEFF}')).ok()?;
        let root = doc
            .get("installed")
            .or_else(|| doc.get("web"))
            .unwrap_or(&doc);
        let id = root.get("client_id")?.as_str()?.trim();
        let secret = root.get("client_secret")?.as_str()?.trim();
        if id.is_empty() || secret.is_empty() {
            None
        } else {
            Some(Self {
                id: id.to_string(),
                secret: secret.to_string(),
            })
        }
    }

    fn load_from_dir(dir: &std::path::Path) -> Option<Self> {
        let exact = dir.join("google-client.json");
        if let Ok(raw) = fs::read_to_string(exact)
            && let Some(c) = Self::parse_json(&raw)
        {
            return Some(c);
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("client_secret") && n.ends_with(".json"))
            })
            .collect();
        entries.sort();
        for p in entries {
            if let Ok(raw) = fs::read_to_string(p)
                && let Some(c) = Self::parse_json(&raw)
            {
                return Some(c);
            }
        }
        None
    }

    pub fn load() -> Option<Self> {
        if let Some(c) = Self::load_from_dir(&AppPaths::data_dir()) {
            return Some(c);
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
            && let Some(c) = Self::load_from_dir(dir)
        {
            return Some(c);
        }
        if let (Ok(id), Ok(secret)) = (
            std::env::var("MOCHI_GOOGLE_CLIENT_ID"),
            std::env::var("MOCHI_GOOGLE_CLIENT_SECRET"),
        ) {
            let id = id.trim();
            let secret = secret.trim();
            if !id.is_empty() && !secret.is_empty() {
                return Some(Self {
                    id: id.to_string(),
                    secret: secret.to_string(),
                });
            }
        }
        if let (Some(id), Some(secret)) = (
            option_env!("MOCHI_GOOGLE_CLIENT_ID"),
            option_env!("MOCHI_GOOGLE_CLIENT_SECRET"),
        ) {
            let id = id.trim();
            let secret = secret.trim();
            if !id.is_empty() && !secret.is_empty() {
                return Some(Self {
                    id: id.to_string(),
                    secret: secret.to_string(),
                });
            }
        }
        None
    }
}

pub struct OAuthSession {
    listener: TcpListener,
    client_id: String,
    client_secret: String,
    verifier: String,
    state: String,
    redirect: String,
}

impl OAuthSession {
    pub fn wait_and_exchange(
        self,
        cancel: Arc<AtomicBool>,
        lang: Language,
    ) -> Result<Value, String> {
        let code = wait_for_oauth_code(&self.listener, &self.state, &cancel, lang)?;
        let form = format!(
            "client_id={}&client_secret={}&code={}&code_verifier={}&grant_type=authorization_code&redirect_uri={}",
            url_encode(&self.client_id),
            url_encode(&self.client_secret),
            url_encode(&code),
            url_encode(&self.verifier),
            url_encode(&self.redirect),
        );
        let resp = http_request(
            "POST",
            "https://oauth2.googleapis.com/token",
            &[("Content-Type", "application/x-www-form-urlencoded")],
            Some(form.as_bytes()),
            20_000,
        )?;
        let doc: Value = serde_json::from_str(&resp.body).map_err(|e| e.to_string())?;
        if !(200..300).contains(&resp.status) {
            let err = doc.get("error").and_then(Value::as_str).unwrap_or("error");
            return Err(err.to_string());
        }
        Ok(doc)
    }
}

pub struct GoogleAuth {
    pub credential: Option<GoogleCredential>,
    pub access_token: Option<String>,
    pub access_expires_utc: Option<DateTime<Utc>>,
    pub needs_reconnect: bool,
    client: Option<GoogleClient>,
}

impl GoogleAuth {
    pub const CALENDAR_LIST_SCOPE: &str =
        "https://www.googleapis.com/auth/calendar.calendarlist.readonly";
    pub const EVENTS_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events.readonly";
    pub const TASKS_SCOPE: &str = "https://www.googleapis.com/auth/tasks";

    pub fn new() -> Self {
        Self {
            credential: SecretFile::read(&AppPaths::google_file()),
            access_token: None,
            access_expires_utc: None,
            needs_reconnect: false,
            client: GoogleClient::load(),
        }
    }

    pub fn reload_client(&mut self) -> bool {
        if self.client.is_none() {
            self.client = GoogleClient::load();
        }
        self.client.is_some()
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub fn is_signed_in(&self) -> bool {
        self.credential.is_some() && !self.needs_reconnect
    }

    pub fn has_scopes(&self, required: &[&str]) -> bool {
        let Some(cred) = &self.credential else {
            return false;
        };
        let granted: Vec<&str> = match &cred.scopes {
            Some(s) => s.iter().map(String::as_str).collect(),
            None => vec![Self::CALENDAR_LIST_SCOPE, Self::EVENTS_SCOPE],
        };
        required.iter().all(|r| granted.contains(r))
    }

    pub fn begin_oauth(&mut self, wanted: &[&str], lang: Language) -> Result<OAuthSession, String> {
        self.reload_client();
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| lang.strings().google_auth_no_client.to_string())?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let redirect = format!("http://127.0.0.1:{port}");

        let verifier = new_pkce_verifier();
        let state = new_pkce_verifier();
        let challenge = mochi_calendar::port::pkce_challenge(verifier.clone())?;
        let mut scopes = vec!["openid", "email"];
        scopes.extend_from_slice(wanted);
        let scope_str = scopes.join(" ");

        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&include_granted_scopes=true&code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
            url_encode(&client.id),
            url_encode(&redirect),
            url_encode(&scope_str),
            url_encode(&challenge),
            url_encode(&state),
        );
        open_url(&auth_url);

        Ok(OAuthSession {
            listener,
            client_id: client.id.clone(),
            client_secret: client.secret.clone(),
            verifier,
            state,
            redirect,
        })
    }

    pub fn finish_oauth(
        &mut self,
        doc: Value,
        wanted: &[&str],
        lang: Language,
    ) -> Result<bool, String> {
        let refresh = doc
            .get("refresh_token")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| lang.strings().google_auth_no_refresh.to_string())?
            .to_string();
        let granted = doc.get("scope").and_then(Value::as_str).unwrap_or("");
        let email = doc
            .get("id_token")
            .and_then(Value::as_str)
            .and_then(|t| {
                mochi_calendar::port::email_from_id_token(Some(t.to_string()))
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

        self.credential = Some(GoogleCredential {
            refresh_token: refresh,
            email,
            scopes: Some(granted.split_whitespace().map(str::to_string).collect()),
        });
        self.needs_reconnect = false;
        self.set_access_token(&doc);
        if let Some(cred) = &self.credential {
            SecretFile::write(&AppPaths::google_file(), cred);
        }
        let req_vec: Vec<String> = wanted.iter().map(|s| (*s).to_string()).collect();
        Ok(mochi_calendar::port::has_o_auth_scopes(granted.to_string(), req_vec).unwrap_or(false))
    }

    pub fn sign_in(
        &mut self,
        wanted: &[&str],
        cancel: Arc<AtomicBool>,
        lang: Language,
    ) -> Result<bool, String> {
        let session = self.begin_oauth(wanted, lang)?;
        let doc = session.wait_and_exchange(cancel, lang)?;
        self.finish_oauth(doc, wanted, lang)
    }

    pub fn sign_out(&mut self) {
        if let Some(cred) = self.credential.take() {
            let form = format!("token={}", url_encode(&cred.refresh_token));
            let _ = http_request(
                "POST",
                "https://oauth2.googleapis.com/revoke",
                &[("Content-Type", "application/x-www-form-urlencoded")],
                Some(form.as_bytes()),
                10_000,
            );
        }
        self.access_token = None;
        self.access_expires_utc = None;
        self.needs_reconnect = false;
        SecretFile::delete(&AppPaths::google_file());
    }

    fn set_access_token(&mut self, doc: &Value) {
        self.access_token = doc
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_string);
        let secs = doc
            .get("expires_in")
            .and_then(Value::as_i64)
            .unwrap_or(3600);
        self.access_expires_utc = Some(Utc::now() + chrono::Duration::seconds((secs - 60).max(60)));
    }

    pub fn get_access_token(&mut self, force_refresh: bool) -> Result<String, String> {
        if !force_refresh
            && let (Some(tok), Some(exp)) = (&self.access_token, self.access_expires_utc)
            && Utc::now() < exp
        {
            return Ok(tok.clone());
        }
        let client = self.client.as_ref().ok_or("No Google client")?;
        let cred = self.credential.as_ref().ok_or("Not connected")?;
        if self.needs_reconnect {
            return Err("Needs reconnect".to_string());
        }
        let form = format!(
            "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
            url_encode(&client.id),
            url_encode(&client.secret),
            url_encode(&cred.refresh_token),
        );
        let resp = http_request(
            "POST",
            "https://oauth2.googleapis.com/token",
            &[("Content-Type", "application/x-www-form-urlencoded")],
            Some(form.as_bytes()),
            20_000,
        )?;
        let doc: Value = serde_json::from_str(&resp.body).map_err(|e| e.to_string())?;
        if !(200..300).contains(&resp.status) {
            let err = doc.get("error").and_then(Value::as_str).unwrap_or("");
            if err == "invalid_grant" {
                self.needs_reconnect = true;
                self.access_token = None;
            }
            return Err(err.to_string());
        }
        self.set_access_token(&doc);
        self.access_token
            .clone()
            .ok_or_else(|| "Missing access_token".to_string())
    }

    pub fn send_api(
        &mut self,
        method: &str,
        url: &str,
        json_body: Option<&str>,
    ) -> Result<HttpResponse, String> {
        for attempt in 0..2 {
            let token = self.get_access_token(attempt > 0)?;
            let auth = format!("Bearer {token}");
            let mut headers = vec![("Authorization", auth.as_str())];
            if json_body.is_some() {
                headers.push(("Content-Type", "application/json"));
            }
            let resp = http_request(method, url, &headers, json_body.map(str::as_bytes), 20_000)?;
            if resp.status == 401 && attempt == 0 {
                continue;
            }
            return Ok(resp);
        }
        Err("Unauthorized".to_string())
    }
}

impl Default for GoogleAuth {
    fn default() -> Self {
        Self::new()
    }
}

fn new_pkce_verifier() -> String {
    let mut bytes = [0u8; 32];
    // SAFETY: `bytes` is a valid 32-byte mutable buffer.
    unsafe {
        let _ = BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    }
    URL_SAFE_NO_PAD.encode(bytes)
}

fn wait_for_oauth_code(
    listener: &TcpListener,
    state: &str,
    cancel: &AtomicBool,
    lang: Language,
) -> Result<String, String> {
    let _ = listener.set_nonblocking(true);
    let deadline = std::time::Instant::now() + Duration::from_secs(300);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(lang.strings().google_auth_cancelled.to_string());
        }
        if std::time::Instant::now() > deadline {
            return Err("Timed out".to_string());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let mut reader = BufReader::new(
                    stream
                        .try_clone()
                        .map_err(|e| e.to_string())?
                        .take(16 * 1024),
                );
                let mut req_line = String::new();
                if reader.read_line(&mut req_line).is_err() {
                    continue;
                }
                for _ in 0..100 {
                    let mut hdr = String::new();
                    if reader.read_line(&mut hdr).is_err() || hdr.trim().is_empty() {
                        break;
                    }
                }
                let parts: Vec<&str> = req_line.split_whitespace().collect();
                let query = if parts.len() >= 2 && parts[0] == "GET" {
                    mochi_calendar::port::parse_o_auth_redirect(parts[1].to_string())
                        .ok()
                        .flatten()
                } else {
                    None
                };
                let got_state = query
                    .as_ref()
                    .and_then(|q| q.get("state"))
                    .and_then(Value::as_str);
                if got_state != Some(state) {
                    let _ = write_http_response(&mut stream, "404 Not Found", "");
                    continue;
                }
                if let Some(code) = query
                    .as_ref()
                    .and_then(|q| q.get("code"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    let html = result_page(
                        lang.strings().google_auth_page_connected,
                        lang.strings().google_auth_page_connected_text,
                        lang.code(),
                    );
                    let _ = write_http_response(&mut stream, "200 OK", &html);
                    return Ok(code.to_string());
                }
                let html = result_page(
                    lang.strings().google_auth_page_cancelled,
                    lang.strings().google_auth_page_cancelled_text,
                    lang.code(),
                );
                let _ = write_http_response(&mut stream, "200 OK", &html);
                return Err(lang.strings().google_auth_cancelled.to_string());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn write_http_response(
    stream: &mut std::net::TcpStream,
    status: &str,
    html: &str,
) -> std::io::Result<()> {
    let body = html.as_bytes();
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    let _ = stream.shutdown(Shutdown::Write);
    Ok(())
}

fn result_page(title: &str, text: &str, lang: &str) -> String {
    format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\"><title>Mochi</title><style>body{{margin:0;height:100vh;display:grid;place-items:center;background:#000;color:#fff;font-family:'Segoe UI Variable Text','Segoe UI',sans-serif}}h1{{font-size:20px;font-weight:600;margin:0 0 6px}}p{{margin:0;color:#ffffff99;font-size:14px}}</style></head><body><main><h1>{}</h1><p>{}</p></main></body></html>",
        html_escape(title),
        html_escape(text)
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::GoogleClient;

    #[test]
    fn parses_installed_web_and_flat_google_client_json() {
        let installed = r#"{"installed":{"client_id":"id1","client_secret":"sec1"}}"#;
        let c1 = GoogleClient::parse_json(installed).expect("installed");
        assert_eq!(c1.id, "id1");
        assert_eq!(c1.secret, "sec1");

        let web = "\u{FEFF}{\"web\":{\"client_id\":\"id2\",\"client_secret\":\"sec2\"}}";
        let c2 = GoogleClient::parse_json(web).expect("web");
        assert_eq!(c2.id, "id2");
        assert_eq!(c2.secret, "sec2");

        let flat = r#"{"client_id":"id3","client_secret":"sec3"}"#;
        let c3 = GoogleClient::parse_json(flat).expect("flat");
        assert_eq!(c3.id, "id3");
        assert_eq!(c3.secret, "sec3");

        assert!(GoogleClient::parse_json(r#"{"installed":{"client_id":""}}"#).is_none());
    }
}
