//! Task 1.4 routing truth: the exact configured provider must serve requests,
//! not just appear in metadata. Spins a local HTTP stub, points config at it,
//! and asserts the request actually arrives with the configured model + key.

use darius_cli::paths::DariusPaths;
use darius_cli::runtime::SessionRuntime;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

const KEY_ENV: &str = "DARIUS_TEST_ROUTING_TRUTH_KEY";
const KEY_VALUE: &str = "routing-truth-secret";

struct SavedEnv {
    key: Option<String>,
    darius: Option<String>,
    openai: Option<String>,
}

impl SavedEnv {
    fn capture() -> Self {
        Self {
            key: std::env::var(KEY_ENV).ok(),
            darius: std::env::var("DARIUS_API_KEY").ok(),
            openai: std::env::var("OPENAI_API_KEY").ok(),
        }
    }

    fn apply_test(&self) {
        unsafe {
            std::env::set_var(KEY_ENV, KEY_VALUE);
            std::env::remove_var("DARIUS_API_KEY");
            std::env::remove_var("OPENAI_API_KEY");
        }
        let _ = self;
    }

    fn restore(self) {
        unsafe {
            match self.key {
                Some(v) => std::env::set_var(KEY_ENV, v),
                None => std::env::remove_var(KEY_ENV),
            }
            match self.darius {
                Some(v) => std::env::set_var("DARIUS_API_KEY", v),
                None => std::env::remove_var("DARIUS_API_KEY"),
            }
            match self.openai {
                Some(v) => std::env::set_var("OPENAI_API_KEY", v),
                None => std::env::remove_var("OPENAI_API_KEY"),
            }
        }
    }
}

#[test]
fn configured_provider_serves_live_requests() {
    let saved = SavedEnv::capture();
    saved.apply_test();

    let result = (|| -> Result<(), String> {
        let temp = TempDir::new().map_err(|e| e.to_string())?;
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&workspace).map_err(|e| e.to_string())?;
        let paths = DariusPaths { home, workspace };

        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            listener.set_nonblocking(true).ok();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let request = (|| -> Result<String, String> {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if std::time::Instant::now() >= deadline {
                                return Err("accept timed out".to_string());
                            }
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
                let mut raw = Vec::new();
                let mut buf = [0_u8; 4096];
                loop {
                    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw).into_owned();
                    if let Some(header_end) = text.find("\r\n\r\n") {
                        let headers = &text[..header_end];
                        let body_len = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                if name.trim().eq_ignore_ascii_case("content-length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0);
                        if text.len() >= header_end + 4 + body_len {
                            break;
                        }
                    }
                    if raw.len() > 1_048_576 {
                        break;
                    }
                }
                let body = serde_json::json!({
                    "choices": [{"message": {"content": "hello-from-local-stub"}}]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream
                    .write_all(response.as_bytes())
                    .map_err(|e| e.to_string())?;
                Ok(String::from_utf8_lossy(&raw).into_owned())
            })();
            let _ = tx.send(request);
        });

        let profile_dir = paths.profile("routed").map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;
        std::fs::write(
            profile_dir.join("config.toml"),
            format!(
                "[model]\nprovider = \"custom-provider\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\nmodel = \"custom-model\"\napi_key_env = \"{KEY_ENV}\"\n"
            ),
        )
        .map_err(|e| e.to_string())?;

        let mut runtime =
            SessionRuntime::from_profile(&paths, "routed").map_err(|e| e.to_string())?;
        assert_eq!(runtime.metadata.mode, "live");
        let reply = runtime.model.react("hi").map_err(|e| e.to_string())?;
        assert!(
            reply.contains("hello-from-local-stub"),
            "model reply should come from local stub, got: {reply}"
        );

        let raw = rx.recv_timeout(Duration::from_secs(15)).map_err(|_| {
            "local stub received no request: configured provider is not routed".to_string()
        })??;
        assert!(
            raw.contains("POST /v1/chat/completions"),
            "stub should see chat completions path, got: {raw}"
        );
        assert!(
            raw.contains(&format!("Bearer {KEY_VALUE}")),
            "stub should see configured key header"
        );
        assert!(
            raw.contains("custom-model"),
            "stub should see configured model name, got: {raw}"
        );
        Ok(())
    })();

    saved.restore();
    assert!(result.is_ok(), "{}", result.err().unwrap_or_default());
}
