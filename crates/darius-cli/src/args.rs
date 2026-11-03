use std::path::PathBuf;

#[derive(clap::Parser)]
#[command(
    name = "darius",
    version,
    about = "Local-first coding agent",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[arg(long, global = true, default_value = "default")]
    pub profile: String,
    #[arg(long, global = true)]
    pub cwd: Option<PathBuf>,
    #[arg(long, global = true)]
    pub offline: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand)]
#[rustfmt::skip]
pub enum Command {
    Tui,
    Run { #[arg(required = true)] goal: Vec<String> },
    Config { #[command(subcommand)] command: ConfigCommand },
    Auth { #[command(subcommand)] command: AuthCommand },
    /// List models for the active profile (or --provider) with source annotation
    Models {
        /// Provider id (e.g. opencode-free, nous); defaults to the active profile's provider
        #[arg(long)]
        provider: Option<String>,
    },
    Memory { #[command(subcommand)] command: MemoryCommand },
    /// Run the Darius web server (CognitiveLoop + SSE)
    Serve {
        /// Host to bind to (default: 127.0.0.1)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind to (default: 7432)
        #[arg(long, default_value_t = 7432)]
        port: u16,
    },
}

#[derive(clap::Subcommand)]
pub enum ConfigCommand {
    /// Display current runtime configuration and diagnostics
    Show,
    /// Probe connectivity and authentication for the configured model provider
    Probe,
    /// Initialize a profile with model provider settings
    Init {
        /// Model provider type (e.g. openai_compatible, ollama)
        #[arg(long)]
        provider: String,
        /// Base URL for the provider API
        #[arg(long)]
        base_url: url::Url,
        /// Model identifier (e.g. gpt-4o-mini, claude-3.5-sonnet, llama3.2)
        #[arg(long)]
        model: String,
        /// Environment variable name holding the API key
        #[arg(long)]
        key_env: String,
        /// Overwrite existing profile configuration
        #[arg(long)]
        force: bool,
    },
    /// Quick-configure a profile from a named preset (openai, openrouter, ollama, groq)
    Preset {
        /// Preset name: openai, openrouter, ollama, or groq
        name: String,
        /// Overwrite existing profile configuration
        #[arg(long)]
        force: bool,
    },
    /// Interactive six-provider setup wizard (key → home .env, nous → OAuth instruction)
    Setup {
        /// Provider id; when absent, an interactive menu is shown (default: opencode-free)
        #[arg(long)]
        provider: Option<String>,
    },
}

#[derive(clap::Subcommand)]
pub enum AuthCommand {
    /// Log in to a provider via OAuth device-code flow (currently: nous)
    Login {
        /// Provider id (currently only `nous`)
        provider: String,
    },
    /// Clear the stored OAuth credential for a provider
    Logout {
        /// Provider id (currently only `nous`)
        provider: String,
    },
    /// Show stored credential metadata (never the token)
    Status,
}

#[derive(clap::Subcommand)]
pub enum MemoryCommand {
    /// Search durable memory records
    Search {
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
    },
    /// Build and display a memory pack
    Pack,
    /// Import memory records from a JSONL file
    Import { file: PathBuf },
    /// Export memory records to a JSONL file
    Export { file: PathBuf },
    /// Display memory database statistics
    Stats,
}
