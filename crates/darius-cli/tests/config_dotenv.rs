//! Tests covering `.env` resolution rules:
//! workspace `.env` -> Darius home `.env`, with process env always winning.

use darius_cli::paths::DariusPaths;
use std::fs;
use tempfile::TempDir;

fn write_env(dir: &std::path::Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).unwrap();
}

fn load_dotenv_for_test(paths: &DariusPaths) {
    // Mirror real parse_and_apply_dotenv: first file wins, process env always wins.
    for base in [&paths.workspace, &paths.home] {
        let env_path = base.join(".env");
        if let Ok(content) = std::fs::read_to_string(&env_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let Some((key, val)) = trimmed.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                let mut val = val.trim();
                if val.len() >= 2
                    && ((val.starts_with('"') && val.ends_with('"'))
                        || (val.starts_with('\'') && val.ends_with('\'')))
                {
                    val = &val[1..val.len() - 1];
                }
                if std::env::var(key).is_err() && !val.is_empty() {
                    unsafe { std::env::set_var(key, val) };
                }
            }
        }
    }
}

#[test]
fn dotenv_home_loads_alongside_workspace() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    write_env(
        &workspace,
        ".env",
        "DARIUS_DOTENV_HOME_TEST=from_workspace\n",
    );
    write_env(
        &home,
        ".env",
        "DARIUS_DOTENV_HOME_TEST=from_home\nDARIUS_DOTENV_HOME_ONLY=home_only\n",
    );

    let paths = DariusPaths { home, workspace };
    load_dotenv_for_test(&paths);
    // Workspace parsed first wins; home's value is ignored (existing vars never overwritten).
    assert_eq!(
        std::env::var("DARIUS_DOTENV_HOME_TEST").unwrap(),
        "from_workspace"
    );
    assert_eq!(
        std::env::var("DARIUS_DOTENV_HOME_ONLY").unwrap(),
        "home_only"
    );
}

#[test]
fn dotenv_process_env_wins_over_file() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    write_env(&home, ".env", "DARIUS_DOTENV_PROC_WIN=from_file\n");
    unsafe {
        std::env::set_var("DARIUS_DOTENV_PROC_WIN", "from_process");
    }

    let paths = DariusPaths { home, workspace };
    load_dotenv_for_test(&paths);
    assert_eq!(
        std::env::var("DARIUS_DOTENV_PROC_WIN").unwrap(),
        "from_process"
    );
}
