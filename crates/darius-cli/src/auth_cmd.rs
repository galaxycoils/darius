//! `darius auth` — Nous Portal OAuth device-code login, logout, status.
//!
//! Tokens live in `<home>/auth.json` (0600, atomic writes) via
//! `darius_daemon::NousAuthService`. Status output never includes the token.

use crate::paths::DariusPaths;
use darius_daemon::{NousAuthError, NousAuthService};

fn service(paths: &DariusPaths) -> Result<NousAuthService, NousAuthError> {
    NousAuthService::new(&paths.home).map_err(NousAuthError::from)
}

pub fn login(paths: &DariusPaths, provider: &str) -> Result<(), String> {
    if provider != "nous" {
        return Err(format!(
            "OAuth login is only supported for provider 'nous' (got '{provider}')"
        ));
    }
    let svc = service(paths).map_err(|e| e.to_string())?;
    let started = crate::runtime::block_on_turn(svc.start_device_code());
    let started = started.map_err(|e| e.to_string())?;
    println!("Visit: {}", started.verification_uri);
    println!("Enter code: {}", started.user_code);
    match crate::runtime::block_on_turn(svc.poll_device_token(&started)) {
        Ok(cred) => {
            println!(
                "Logged in to nous (scope: {}).",
                cred.scope.as_deref().unwrap_or("inference:invoke")
            );
            Ok(())
        }
        Err(e) => Err(format!("Login failed: {e}")),
    }
}

pub fn logout(paths: &DariusPaths, provider: &str) -> Result<(), String> {
    if provider != "nous" {
        return Err(format!(
            "OAuth logout is only supported for provider 'nous' (got '{provider}')"
        ));
    }
    service(paths)
        .map_err(|e| e.to_string())?
        .delete()
        .map_err(|e| e.to_string())?;
    println!("Logged out of nous.");
    Ok(())
}

pub fn status(paths: &DariusPaths) -> Result<(), String> {
    let svc = service(paths).map_err(|e| e.to_string())?;
    match svc.status().map_err(|e| e.to_string())? {
        None => {
            println!("nous: not logged in (run `darius auth login nous`)");
            Ok(())
        }
        Some(s) => {
            println!("provider: {}", s.provider);
            println!("token type: {}", s.token_type);
            match s.expires_at {
                Some(exp) => println!("expires at (unix): {exp}"),
                None => println!("expires at: unknown"),
            }
            println!("quarantined: {}", s.quarantined);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths() -> (tempfile::TempDir, DariusPaths) {
        let temp = tempfile::TempDir::new().unwrap();
        // Build directly: resolve() would read the real $HOME/.darius
        // (DARIUS_HOME unset) and leak user credentials into tests.
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let paths = DariusPaths {
            home,
            workspace: temp.path().to_path_buf(),
        };
        (temp, paths)
    }

    #[test]
    fn auth_status_empty_reports_not_logged_in() {
        let (_t, paths) = test_paths();
        assert!(status(&paths).is_ok());
        let svc = service(&paths).unwrap();
        assert!(svc.status().unwrap().is_none());
    }

    #[test]
    fn auth_rejects_non_nous_provider() {
        let (_t, paths) = test_paths();
        assert!(login(&paths, "openrouter").is_err());
        assert!(logout(&paths, "openrouter").is_err());
    }

    #[test]
    fn auth_logout_clears_stored_credential() {
        let (_t, paths) = test_paths();
        let svc = service(&paths).unwrap();
        svc.save(&darius_daemon::NousCred {
            access_token: "test-token".into(),
            refresh_token: None,
            expires_at: None,
            scope: None,
            token_type: Some("Bearer".into()),
            quarantined: false,
            quarantined_at: None,
            quarantine_reason: None,
        })
        .unwrap();
        assert!(logout(&paths, "nous").is_ok());
        assert!(svc.status().unwrap().is_none());
    }

    #[tokio::test]
    async fn auth_device_code_login_against_fake_portal() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/device/code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "dev-123",
                "user_code": "ABCD-1234",
                "verification_uri": "https://portal.example/activate",
                "expires_in": 600,
                "interval": 1
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "fake-jwt",
                "refresh_token": "fake-refresh",
                "expires_in": 3600,
                "token_type": "Bearer",
                "scope": "inference:invoke"
            })))
            .mount(&server)
            .await;
        unsafe {
            std::env::set_var("NOUS_PORTAL_BASE_URL", server.uri());
            std::env::set_var("NOUS_INFERENCE_BASE_URL", server.uri());
        }
        let (_t, paths) = test_paths();
        let svc = service(&paths).unwrap();
        let started = svc.start_device_code().await.unwrap();
        assert_eq!(started.user_code, "ABCD-1234");
        let cred = svc.poll_device_token(&started).await.unwrap();
        assert_eq!(cred.access_token, "fake-jwt");
        // Stored: status sees it, access token never printed by status().
        assert!(svc.status().unwrap().is_some());
        unsafe {
            std::env::remove_var("NOUS_PORTAL_BASE_URL");
            std::env::remove_var("NOUS_INFERENCE_BASE_URL");
        }
    }
}
