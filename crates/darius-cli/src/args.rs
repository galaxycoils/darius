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
    Memory { #[command(subcommand)] command: MemoryCommand },
}

#[derive(clap::Subcommand)]
pub enum ConfigCommand {
    Show,
    Init {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        base_url: url::Url,
        #[arg(long)]
        model: String,
        #[arg(long)]
        key_env: String,
        #[arg(long)]
        force: bool,
    },
}

#[derive(clap::Subcommand)]
#[rustfmt::skip]
pub enum MemoryCommand {
    Search { #[arg(required = true, num_args = 1..)] query: Vec<String> },
    Pack,
    Import { file: PathBuf },
    Export { file: PathBuf },
    Stats,
}
