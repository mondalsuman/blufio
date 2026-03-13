// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blufio - An always-on personal AI agent.
//!
//! This is the binary entry point for the Blufio agent.

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

mod backup;
mod bench;
mod bundle;
mod classify;
mod cli;
mod context;
mod cron_cmd;
mod doctor;
mod encrypt;
mod gdpr_cmd;
mod healthcheck;
#[allow(dead_code)]
mod hot_reload;
mod litestream;
#[cfg(feature = "mcp-server")]
mod mcp_server;
mod migrate;
mod otel;
mod pii_cmd;
mod privacy;
mod providers;
mod serve;
mod shell;
mod status;
mod uninstall;
mod update;
mod verify;

use clap::{Parser, Subcommand};

/// Blufio - An always-on personal AI agent.
#[derive(Parser, Debug)]
#[command(name = "blufio", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

/// Available subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the Blufio agent server.
    Serve,
    /// Launch an interactive REPL session.
    Shell,
    /// Show agent status (connects to health endpoint).
    Status {
        /// Output as structured JSON for scripting.
        #[arg(long)]
        json: bool,
        /// Disable colored output.
        #[arg(long)]
        plain: bool,
    },
    /// Run diagnostic checks against the environment.
    Doctor {
        /// Run additional intensive checks (DB integrity, memory, disk).
        #[arg(long)]
        deep: bool,
        /// Disable colored output.
        #[arg(long)]
        plain: bool,
    },
    /// Create an atomic backup of the SQLite database.
    Backup {
        /// Destination path for the backup file.
        path: String,
    },
    /// Restore the database from a backup file.
    Restore {
        /// Path to the backup file to restore from.
        path: String,
    },
    /// Manage Blufio configuration and vault secrets.
    Config {
        #[command(subcommand)]
        action: Option<ConfigCommands>,
    },
    /// Manage Blufio skills (WASM plugins).
    Skill {
        #[command(subcommand)]
        action: SkillCommands,
    },
    /// Manage Blufio plugins (compiled-in adapter modules).
    Plugin {
        #[command(subcommand)]
        action: PluginCommands,
    },
    /// Start the MCP server on stdio (for Claude Desktop integration).
    #[command(name = "mcp-server")]
    McpServer,
    /// Database management commands.
    Db {
        #[command(subcommand)]
        action: DbCommands,
    },
    /// Verify a file's Minisign signature.
    Verify {
        /// Path to the file to verify.
        file: String,
        /// Path to the .minisig signature file (auto-detected if omitted).
        #[arg(long)]
        signature: Option<String>,
    },
    /// Update Blufio to the latest version.
    Update {
        #[command(subcommand)]
        action: Option<UpdateCommands>,
        /// Skip interactive confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Migrate data from another AI agent platform.
    Migrate {
        #[command(subcommand)]
        action: MigrateCommands,
    },
    /// Run a health check (for Docker HEALTHCHECK). Exits 0 if healthy, 1 if not.
    Healthcheck,
    /// Manage paired device nodes.
    #[cfg(feature = "node")]
    Nodes {
        #[command(subcommand)]
        action: NodesCommands,
    },
    /// Run built-in performance benchmarks.
    Bench {
        /// Run only specific benchmarks (comma-separated: startup,sqlite,wasm,context).
        #[arg(long)]
        only: Option<String>,
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
        /// Compare results against previous run.
        #[arg(long)]
        compare: bool,
        /// Save current results as baseline.
        #[arg(long)]
        baseline: bool,
        /// Number of iterations per benchmark (default: 3).
        #[arg(long)]
        iterations: Option<u32>,
        /// CI mode: exit non-zero if benchmarks regress beyond threshold.
        #[arg(long)]
        ci: bool,
        /// Regression threshold percentage for CI mode (default: 20).
        #[arg(long)]
        threshold: Option<f64>,
    },
    /// Generate a privacy evidence report.
    Privacy {
        #[command(subcommand)]
        action: PrivacyCommands,
    },
    /// Create an air-gapped deployment bundle.
    Bundle {
        /// Output path for the archive (default: blufio-{version}-{platform}.tar.gz).
        #[arg(long)]
        output: Option<String>,
        /// Include SQLite database backup in bundle.
        #[arg(long)]
        include_data: bool,
    },
    /// Uninstall Blufio (remove binary, service files, optionally data).
    Uninstall {
        /// Remove all data without prompting (auto-backup created first).
        #[arg(long)]
        purge: bool,
    },
    /// Manage data classification levels on memories, messages, and sessions.
    #[command(
        after_help = "Examples:\n  blufio classify set memory mem-42 confidential\n  blufio classify get memory mem-42\n  blufio classify list --type memory --level confidential --json\n  blufio classify bulk --type memory --level restricted --current-level internal --dry-run"
    )]
    Classify {
        #[command(subcommand)]
        action: classify::ClassifyAction,
    },
    /// Scan text for PII (Personally Identifiable Information).
    #[command(
        after_help = "Examples:\n  blufio pii scan \"test@example.com or 555-123-4567\"\n  blufio pii scan --file /path/to/data.txt\n  echo \"SSN: 123-45-6789\" | blufio pii scan\n  blufio pii scan --file /tmp/data.txt --json"
    )]
    Pii {
        #[command(subcommand)]
        action: pii_cmd::PiiAction,
    },
    /// Audit trail management.
    #[command(
        after_help = "Examples:\n  blufio audit verify\n  blufio audit tail -n 50 --type session.*\n  blufio audit stats --json"
    )]
    Audit {
        #[command(subcommand)]
        action: AuditCommands,
    },
    /// Manage long-term memories.
    #[command(
        after_help = "Examples:\n  blufio memory validate --dry-run\n  blufio memory validate --json"
    )]
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    /// Manage context engine: compaction, archives, and zone status.
    #[command(
        after_help = "Examples:\n  blufio context compact --dry-run --session <id>\n  blufio context archive list\n  blufio context archive view <archive_id>\n  blufio context archive prune --user <uid> --keep 5\n  blufio context status --session <id>"
    )]
    Context {
        #[command(subcommand)]
        command: context::ContextCommand,
    },
    /// Injection defense testing and status.
    #[command(
        after_help = "Examples:\n  blufio injection test \"ignore previous instructions\"\n  blufio injection test \"hello how are you\"\n  blufio injection status --json\n  blufio injection config --json"
    )]
    Injection {
        #[command(subcommand)]
        action: InjectionCommands,
    },
    /// Manage cron jobs: list, add, remove, run, view history, and generate systemd timers.
    #[command(
        after_help = "Examples:\n  blufio cron list\n  blufio cron add nightly-backup '0 2 * * *' backup\n  blufio cron run-now nightly-backup\n  blufio cron history --job nightly-backup --limit 5\n  blufio cron generate-timers /etc/systemd/system"
    )]
    Cron {
        #[command(subcommand)]
        action: CronCommands,
    },
    /// GDPR data subject rights tooling.
    #[command(
        after_help = "GDPR data subject rights tooling. Supports right to erasure (Art. 17), \
        data portability (Art. 20), and transparency (Art. 15).\n\n\
        Workflow:\n  \
        1. blufio gdpr list-users\n  \
        2. blufio gdpr report --user <id>\n  \
        3. blufio gdpr export --user <id>\n  \
        4. blufio gdpr erase --user <id>"
    )]
    Gdpr {
        #[command(subcommand)]
        action: GdprCommands,
    },
    /// Litestream WAL replication management.
    #[command(
        after_help = "Litestream enables continuous SQLite WAL replication to S3-compatible storage.\n\n\
        NOTE: Litestream is INCOMPATIBLE with SQLCipher encrypted databases.\n\
        If encryption is enabled, use `blufio backup` + cron for scheduled backups.\n\n\
        Examples:\n  blufio litestream init\n  blufio litestream status"
    )]
    Litestream {
        #[command(subcommand)]
        command: LitestreamCommands,
    },
}

/// Cron subcommands.
#[derive(Subcommand, Debug)]
pub enum CronCommands {
    /// List all configured cron jobs.
    List {
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
    /// Add a new cron job.
    Add {
        /// Unique job name.
        name: String,
        /// Cron expression (5-field).
        schedule: String,
        /// Task to execute (must be a registered task name).
        task: String,
    },
    /// Remove a cron job.
    Remove {
        /// Job name to remove.
        name: String,
    },
    /// Run a cron job immediately.
    #[command(name = "run-now")]
    RunNow {
        /// Job name to run.
        name: String,
    },
    /// Show job execution history.
    History {
        /// Filter by job name.
        #[arg(long)]
        job: Option<String>,
        /// Maximum number of entries to show.
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
    /// Generate systemd timer and service unit files for all enabled cron jobs.
    #[command(name = "generate-timers")]
    GenerateTimers {
        /// Output directory for generated files.
        output_dir: String,
    },
}

/// GDPR subcommands.
#[derive(Subcommand, Debug)]
pub enum GdprCommands {
    /// Delete all data for a specific user (GDPR Art. 17 Right to Erasure).
    Erase {
        /// User ID to erase data for.
        #[arg(long)]
        user: String,
        /// Skip interactive confirmation.
        #[arg(long)]
        yes: bool,
        /// Preview deletion counts without actually deleting.
        #[arg(long)]
        dry_run: bool,
        /// Skip automatic export before erasure.
        #[arg(long)]
        skip_export: bool,
        /// Force erasure even if user has active sessions.
        #[arg(long)]
        force: bool,
        /// Timeout in seconds (default: 300).
        #[arg(long, default_value = "300")]
        timeout: u64,
    },
    /// Generate a transparency report of held data (GDPR Art. 15).
    Report {
        /// User ID to report on.
        #[arg(long)]
        user: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Export user data in JSON or CSV format (GDPR Art. 20 Data Portability).
    Export {
        /// User ID to export data for.
        #[arg(long)]
        user: String,
        /// Export format: json or csv.
        #[arg(long, default_value = "json")]
        format: String,
        /// Filter to specific session ID.
        #[arg(long)]
        session: Option<String>,
        /// Include data from this timestamp (ISO 8601).
        #[arg(long)]
        since: Option<String>,
        /// Include data until this timestamp (ISO 8601).
        #[arg(long)]
        until: Option<String>,
        /// Filter to specific data types (comma-separated: messages,memories,sessions,cost_records).
        #[arg(long, value_delimiter = ',')]
        r#type: Option<Vec<String>>,
        /// Apply PII redaction to exported data.
        #[arg(long)]
        redact: bool,
        /// Custom output file path.
        #[arg(long)]
        output: Option<String>,
    },
    /// List all users with data in the system.
    ListUsers {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Litestream subcommands.
#[derive(Subcommand, Debug)]
pub enum LitestreamCommands {
    /// Generate Litestream config template alongside database file.
    Init,
    /// Check Litestream replication status and lag.
    Status,
}

/// Privacy subcommands.
#[derive(Subcommand, Debug)]
enum PrivacyCommands {
    /// Generate a privacy evidence report.
    EvidenceReport {
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
        /// Save report to file instead of printing.
        #[arg(long)]
        output: Option<String>,
    },
}

/// Update subcommands.
#[derive(Subcommand, Debug)]
enum UpdateCommands {
    /// Check for available updates without downloading.
    Check,
    /// Rollback to the pre-update binary.
    Rollback,
}

/// Migration subcommands.
#[derive(Subcommand, Debug)]
enum MigrateCommands {
    /// Import data from OpenClaw.
    #[command(name = "--from-openclaw")]
    FromOpenclaw {
        /// Path to OpenClaw data directory (auto-detected if omitted).
        #[arg(long)]
        data_dir: Option<String>,
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
    /// Preview what would be imported (dry run).
    Preview {
        /// Path to OpenClaw data directory.
        #[arg(long)]
        data_dir: Option<String>,
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Config management subcommands.
#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Store or update an encrypted secret in the vault.
    SetSecret {
        /// The name/key for the secret (e.g., "anthropic.api_key").
        key: String,
    },
    /// List all secrets stored in the vault (names and masked previews only).
    ListSecrets,
    /// Get the current resolved value for a config key (dotted path).
    Get {
        /// Config key path (e.g., "agent.name", "storage.database_path").
        key: String,
    },
    /// Validate the configuration file and report any errors.
    Validate,
    /// Translate an OpenClaw JSON config to Blufio TOML.
    Translate {
        /// Path to OpenClaw JSON config file.
        input: String,
        /// Output file path (prints to stdout if omitted).
        #[arg(long)]
        output: Option<String>,
    },
    /// Generate a config template for a specific use case.
    Recipe {
        /// Preset: personal, team, production, or iot.
        preset: String,
    },
}

/// Skill management subcommands.
#[derive(Subcommand, Debug)]
enum SkillCommands {
    /// Create a new skill project scaffold.
    Init {
        /// Name of the skill to create.
        name: String,
    },
    /// List all installed skills.
    List,
    /// Install a WASM skill from a file.
    Install {
        /// Path to the .wasm file.
        wasm_path: String,
        /// Path to the skill.toml manifest.
        manifest_path: String,
    },
    /// Remove an installed skill.
    Remove {
        /// Name of the skill to remove.
        name: String,
    },
    /// Update an installed skill from a new WASM file.
    Update {
        /// Name of the installed skill to update.
        name: String,
        /// Path to the new .wasm file.
        wasm_path: String,
        /// Path to the updated skill.toml manifest.
        manifest_path: String,
    },
    /// Sign a WASM skill artifact with an Ed25519 private key.
    Sign {
        /// Path to the .wasm file to sign.
        wasm_path: String,
        /// Path to the publisher's private key file.
        private_key_path: String,
    },
    /// Generate a new Ed25519 publisher keypair for skill signing.
    Keygen {
        /// Output directory for the keypair files.
        #[arg(default_value = ".")]
        output_dir: String,
    },
    /// Verify an installed skill's hash and signature.
    Verify {
        /// Name of the installed skill to verify.
        name: String,
    },
    /// Show detailed information about an installed skill.
    Info {
        /// Name of the installed skill to inspect.
        name: String,
    },
}

/// Plugin management subcommands.
#[derive(Subcommand, Debug)]
enum PluginCommands {
    /// List all compiled-in plugins and their status.
    List,
    /// Search available plugins in the built-in catalog.
    Search {
        /// Search query (matches name or description).
        #[arg(default_value = "")]
        query: String,
    },
    /// Enable a plugin (set enabled in config).
    Install {
        /// Plugin name to enable.
        name: String,
    },
    /// Disable a plugin (set disabled in config).
    Remove {
        /// Plugin name to disable.
        name: String,
    },
    /// Show plugin update information.
    Update,
}

/// Node management subcommands.
#[cfg(feature = "node")]
#[derive(Subcommand, Debug)]
enum NodesCommands {
    /// List all paired nodes with status information.
    List {
        /// Output as JSON instead of table.
        #[arg(long)]
        json: bool,
    },
    /// Initiate pairing with another Blufio instance.
    Pair {
        /// Use CLI token mode instead of QR code.
        #[arg(long)]
        token: bool,
    },
    /// Remove a paired node.
    Remove {
        /// Node ID to remove.
        node_id: String,
    },
    /// Manage node groups.
    Group {
        #[command(subcommand)]
        action: NodeGroupCommands,
    },
    /// Execute a command on one or more nodes.
    Exec {
        /// Target nodes or groups (comma-separated).
        #[arg(long, value_delimiter = ',')]
        targets: Vec<String>,
        /// Command and arguments (after --).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
}

/// Node group subcommands.
#[cfg(feature = "node")]
#[derive(Subcommand, Debug)]
enum NodeGroupCommands {
    /// Create a new node group.
    Create {
        /// Group name.
        name: String,
        /// Comma-separated node IDs.
        #[arg(long, value_delimiter = ',')]
        nodes: Vec<String>,
    },
    /// Delete a node group.
    Delete {
        /// Group name.
        name: String,
    },
    /// List all groups.
    List,
}

/// Audit trail subcommands.
#[derive(Subcommand, Debug)]
enum AuditCommands {
    /// Verify the audit hash chain integrity.
    #[command(
        after_help = "Examples:\n  blufio audit verify\n  blufio audit verify --json\n\nChecks the SHA-256 hash chain for breaks, ID gaps, and GDPR-erased entries.\nExit code: 0 = intact, 1 = broken."
    )]
    Verify {
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show recent audit trail entries.
    #[command(
        after_help = "Examples:\n  blufio audit tail\n  blufio audit tail -n 50\n  blufio audit tail --type session.*\n  blufio audit tail --since 2026-03-01 --until 2026-03-10\n  blufio audit tail --actor user:123 --json"
    )]
    Tail {
        /// Number of entries to show (default: 20).
        #[arg(short, long, default_value_t = 20)]
        n: usize,
        /// Filter by event type (prefix match with .*, or exact match).
        #[arg(long, name = "type")]
        event_type: Option<String>,
        /// Filter entries on or after this timestamp (ISO 8601).
        #[arg(long)]
        since: Option<String>,
        /// Filter entries on or before this timestamp (ISO 8601).
        #[arg(long)]
        until: Option<String>,
        /// Filter by actor prefix (e.g., "user:123").
        #[arg(long)]
        actor: Option<String>,
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show audit trail statistics.
    #[command(after_help = "Examples:\n  blufio audit stats\n  blufio audit stats --json")]
    Stats {
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Memory management subcommands.
#[derive(Subcommand, Debug)]
enum MemoryCommand {
    /// Validate memory index: detect duplicates, stale entries, and conflicts.
    Validate {
        /// Preview only -- do not modify any memories.
        #[arg(long)]
        dry_run: bool,
        /// Output results as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Drop and rebuild the vec0 virtual table from the memories table.
    #[command(name = "rebuild-vec0")]
    RebuildVec0,
}

/// Injection defense subcommands.
#[derive(Subcommand, Debug)]
enum InjectionCommands {
    /// Test text against injection detection patterns.
    Test {
        /// Text to scan.
        text: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Disable colored output.
        #[arg(long, alias = "no-color")]
        plain: bool,
    },
    /// Show injection defense status (config + layer info).
    Status {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show effective injection defense configuration.
    Config {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Database management subcommands.
#[derive(Subcommand, Debug)]
enum DbCommands {
    /// Encrypt an existing plaintext database with SQLCipher.
    Encrypt {
        /// Skip interactive confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Generate a random 256-bit encryption key (hex-encoded).
    Keygen,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Load and validate configuration at startup
    let config = match blufio_config::load_and_validate() {
        Ok(config) => {
            eprintln!("blufio: config loaded (agent.name={})", config.agent.name);
            config
        }
        Err(errors) => {
            blufio_config::render_errors(&errors);
            std::process::exit(1);
        }
    };

    match cli.command {
        Some(Commands::Serve) => {
            if let Err(e) = serve::run_serve(config).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Shell) => {
            if let Err(e) = shell::run_shell(config).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Status { json, plain }) => {
            if let Err(e) = status::run_status(&config, json, plain).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Doctor { deep, plain }) => {
            if let Err(e) = doctor::run_doctor(&config, deep, plain).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Backup { path }) => {
            if let Err(e) = backup::run_backup(&config.storage.database_path, &path) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Restore { path }) => {
            if let Err(e) = backup::run_restore(&config.storage.database_path, &path) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Config { action }) => match action {
            Some(ConfigCommands::SetSecret { key }) => {
                if let Err(e) = cli::config_cmd::cmd_set_secret(&config, &key).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::ListSecrets) => {
                if let Err(e) = cli::config_cmd::cmd_list_secrets(&config).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::Get { key }) => {
                if let Err(e) = cli::config_cmd::cmd_config_get(&config, &key) {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::Validate) => match blufio_config::load_and_validate() {
                Ok(_) => {
                    println!("Configuration is valid.");
                }
                Err(errors) => {
                    blufio_config::render_errors(&errors);
                    std::process::exit(1);
                }
            },
            Some(ConfigCommands::Translate { input, output }) => {
                if let Err(e) = migrate::run_config_translate(&input, output.as_deref()) {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::Recipe { preset }) => {
                let recipe = cli::config_cmd::generate_config_recipe(&preset);
                match recipe {
                    Ok(content) => println!("{content}"),
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            None => {
                println!("blufio config: use --help for available config commands");
            }
        },
        Some(Commands::Skill { action }) => {
            if let Err(e) = cli::skill_cmd::handle_skill_command(&config, action).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Plugin { action }) => {
            if let Err(e) = cli::plugin_cmd::handle_plugin_command(&config, action) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Db { action }) => match action {
            DbCommands::Encrypt { yes } => {
                if let Err(e) = encrypt::run_encrypt(&config.storage.database_path, yes) {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            DbCommands::Keygen => {
                encrypt::run_keygen();
            }
        },
        Some(Commands::Verify { file, signature }) => {
            if let Err(e) = verify::run_verify(&file, signature.as_deref()) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Update { action, yes }) => match action {
            Some(UpdateCommands::Check) => {
                if let Err(e) = update::run_check().await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(UpdateCommands::Rollback) => {
                if let Err(e) = update::run_rollback() {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            None => {
                if let Err(e) = update::run_update(yes).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        },
        Some(Commands::McpServer) => {
            #[cfg(feature = "mcp-server")]
            {
                if let Err(e) = mcp_server::run_mcp_server(config).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(feature = "mcp-server"))]
            {
                eprintln!("error: blufio was compiled without mcp-server feature");
                std::process::exit(1);
            }
        }
        Some(Commands::Migrate { action }) => match action {
            MigrateCommands::FromOpenclaw { data_dir, json } => {
                if let Err(e) = migrate::run_migrate(&config, data_dir.as_deref(), json).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            MigrateCommands::Preview { data_dir, json } => {
                if let Err(e) = migrate::run_migrate_preview(data_dir.as_deref(), json).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        },
        Some(Commands::Healthcheck) => {
            if let Err(_e) = healthcheck::run_healthcheck(&config).await {
                std::process::exit(1);
            }
        }
        #[cfg(feature = "node")]
        Some(Commands::Nodes { action }) => {
            if let Err(e) = cli::nodes_cmd::handle_nodes_command(&config, action).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Bench {
            only,
            json,
            compare,
            baseline,
            iterations,
            ci,
            threshold,
        }) => {
            let only_list = only.map(|s| vec![s]);
            if let Err(e) = bench::run_bench(
                only_list, json, compare, baseline, iterations, ci, threshold,
            )
            .await
            {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Privacy { action }) => match action {
            PrivacyCommands::EvidenceReport { json, output } => {
                if let Err(e) = privacy::run_privacy_report(json, output.as_deref()).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        },
        Some(Commands::Bundle {
            output,
            include_data,
        }) => {
            if let Err(e) = bundle::run_bundle(output.as_deref(), include_data) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Uninstall { purge }) => {
            if let Err(e) = uninstall::run_uninstall(purge).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Classify { action }) => {
            if let Err(e) = classify::run_classify(action).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Pii { action }) => {
            if let Err(e) = pii_cmd::run_pii(action).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Audit { action }) => {
            // Resolve audit.db path: config override or default beside main DB.
            let audit_db_path = config.audit.db_path.clone().unwrap_or_else(|| {
                let db = std::path::Path::new(&config.storage.database_path);
                db.parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join("audit.db")
                    .to_string_lossy()
                    .to_string()
            });

            if !config.audit.enabled {
                eprintln!("Note: Audit trail is disabled in config. Showing existing data.");
            }

            match action {
                AuditCommands::Verify { json } => {
                    cli::audit_cmd::run_audit_verify(&audit_db_path, json);
                }
                AuditCommands::Tail {
                    n,
                    event_type,
                    since,
                    until,
                    actor,
                    json,
                } => {
                    cli::audit_cmd::run_audit_tail(
                        &audit_db_path,
                        n,
                        event_type,
                        since,
                        until,
                        actor,
                        json,
                    );
                }
                AuditCommands::Stats { json } => {
                    cli::audit_cmd::run_audit_stats(&audit_db_path, json);
                }
            }
        }
        Some(Commands::Memory { command }) => {
            if let Err(e) = cli::memory_cmd::handle_memory_command(&config, command).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Context { command }) => {
            if let Err(e) = context::run_context(&config, command).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Injection { action }) => {
            cli::injection_cmd::run_injection_command(&config, action);
        }
        Some(Commands::Cron { action }) => {
            if let Err(e) =
                cron_cmd::handle_cron_command(action, &config.storage.database_path).await
            {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Gdpr { action }) => {
            if let Err(e) = gdpr_cmd::handle_gdpr_command(action, &config).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Litestream { command }) => match command {
            LitestreamCommands::Init => {
                if let Err(e) = litestream::run_litestream_init(&config) {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            LitestreamCommands::Status => {
                if let Err(e) = litestream::run_litestream_status(&config) {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        },
        None => {
            println!("blufio: use --help for available commands");
        }
    }
}

// Handler implementations live in cli/ modules:
// - cli::audit_cmd (audit verify/tail/stats)
// - cli::config_cmd (set-secret, list-secrets, config get, recipes)
// - cli::skill_cmd (skill init/list/install/remove/update/sign/keygen/verify/info)
// - cli::memory_cmd (memory validate)
// - cli::plugin_cmd (plugin list/search/install/remove/update)
// - cli::nodes_cmd (nodes list/pair/remove/group/exec)
// - cli::injection_cmd (injection test/status/config)

// Previously-inline handler functions have been moved to cli/ modules.

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(not(target_env = "msvc"))]
    fn jemalloc_is_active() {
        // Verify jemalloc is the global allocator by advancing the epoch.
        // Only jemalloc supports this -- the system allocator would fail.
        use tikv_jemalloc_ctl::{epoch, stats};
        epoch::advance().unwrap();
        let allocated = stats::allocated::read().unwrap();
        assert!(allocated > 0, "jemalloc should report non-zero allocation");
    }

    #[test]
    fn binary_loads_config_defaults() {
        // Verify config loads with defaults (no config file needed)
        let config = blufio_config::load_and_validate().expect("default config should be valid");
        assert_eq!(config.agent.name, "blufio");
    }

    use super::*;
    use blufio_config::model::BlufioConfig;

    #[test]
    fn cli_parses_set_secret_subcommand() {
        let cli = Cli::parse_from(["blufio", "config", "set-secret", "my-key"]);
        match cli.command {
            Some(Commands::Config {
                action: Some(ConfigCommands::SetSecret { key }),
            }) => {
                assert_eq!(key, "my-key");
            }
            _ => panic!("expected Config SetSecret command"),
        }
    }

    #[test]
    fn cli_parses_list_secrets_subcommand() {
        let cli = Cli::parse_from(["blufio", "config", "list-secrets"]);
        match cli.command {
            Some(Commands::Config {
                action: Some(ConfigCommands::ListSecrets),
            }) => {}
            _ => panic!("expected Config ListSecrets command"),
        }
    }

    #[test]
    fn cli_config_without_subcommand() {
        let cli = Cli::parse_from(["blufio", "config"]);
        match cli.command {
            Some(Commands::Config { action: None }) => {}
            _ => panic!("expected Config with no subcommand"),
        }
    }

    #[tokio::test]
    async fn set_secret_and_list_secrets_roundtrip() {
        use secrecy::ExposeSecret;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test-cli.db");

        let config = BlufioConfig {
            storage: blufio_config::model::StorageConfig {
                database_path: db_path.to_str().unwrap().to_string(),
                ..Default::default()
            },
            vault: blufio_config::model::VaultConfig {
                kdf_memory_cost: 32768,
                kdf_iterations: 2,
                kdf_parallelism: 1,
            },
            ..Default::default()
        };

        // Set passphrase via env var for test.
        unsafe { std::env::set_var("BLUFIO_VAULT_KEY", "test-cli-pass") };

        // Open DB and create vault manually (since we can't pipe stdin in test).
        let db = cli::config_cmd::open_db(&config).await.unwrap();
        let conn = db.connection().clone();
        let passphrase = secrecy::SecretString::from("test-cli-pass".to_string());
        let vault = blufio_vault::Vault::create(conn, &passphrase, &config.vault)
            .await
            .unwrap();

        // Store a secret directly.
        vault
            .store_secret("test.api_key", "sk-test-12345678")
            .await
            .unwrap();

        // Verify retrieval.
        let retrieved = vault
            .retrieve_secret("test.api_key")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.expose_secret(), "sk-test-12345678");

        // Verify list shows masked preview.
        let secrets = vault.list_secrets().await.unwrap();
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].0, "test.api_key");
        assert!(secrets[0].1.contains("..."));
        assert!(!secrets[0].1.contains("sk-test-12345678"));

        db.close().await.unwrap();

        unsafe { std::env::remove_var("BLUFIO_VAULT_KEY") };
    }

    #[tokio::test]
    async fn list_secrets_no_vault_graceful() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test-no-vault.db");

        let config = BlufioConfig {
            storage: blufio_config::model::StorageConfig {
                database_path: db_path.to_str().unwrap().to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        // This should succeed gracefully -- no vault exists.
        let result = cli::config_cmd::cmd_list_secrets(&config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parses_status() {
        let cli = Cli::parse_from(["blufio", "status"]);
        match cli.command {
            Some(Commands::Status { json, plain }) => {
                assert!(!json);
                assert!(!plain);
            }
            _ => panic!("expected Status command"),
        }
    }

    #[test]
    fn cli_parses_status_json() {
        let cli = Cli::parse_from(["blufio", "status", "--json"]);
        match cli.command {
            Some(Commands::Status { json, plain }) => {
                assert!(json);
                assert!(!plain);
            }
            _ => panic!("expected Status --json command"),
        }
    }

    #[test]
    fn cli_parses_status_plain() {
        let cli = Cli::parse_from(["blufio", "status", "--plain"]);
        match cli.command {
            Some(Commands::Status { json, plain }) => {
                assert!(!json);
                assert!(plain);
            }
            _ => panic!("expected Status --plain command"),
        }
    }

    #[test]
    fn cli_parses_doctor() {
        let cli = Cli::parse_from(["blufio", "doctor"]);
        match cli.command {
            Some(Commands::Doctor { deep, plain }) => {
                assert!(!deep);
                assert!(!plain);
            }
            _ => panic!("expected Doctor command"),
        }
    }

    #[test]
    fn cli_parses_doctor_deep() {
        let cli = Cli::parse_from(["blufio", "doctor", "--deep"]);
        match cli.command {
            Some(Commands::Doctor { deep, plain }) => {
                assert!(deep);
                assert!(!plain);
            }
            _ => panic!("expected Doctor --deep command"),
        }
    }

    #[test]
    fn cli_parses_backup() {
        let cli = Cli::parse_from(["blufio", "backup", "/tmp/backup.db"]);
        match cli.command {
            Some(Commands::Backup { path }) => {
                assert_eq!(path, "/tmp/backup.db");
            }
            _ => panic!("expected Backup command"),
        }
    }

    #[test]
    fn cli_parses_restore() {
        let cli = Cli::parse_from(["blufio", "restore", "/tmp/backup.db"]);
        match cli.command {
            Some(Commands::Restore { path }) => {
                assert_eq!(path, "/tmp/backup.db");
            }
            _ => panic!("expected Restore command"),
        }
    }

    #[test]
    fn cli_parses_config_get() {
        let cli = Cli::parse_from(["blufio", "config", "get", "agent.name"]);
        match cli.command {
            Some(Commands::Config {
                action: Some(ConfigCommands::Get { key }),
            }) => {
                assert_eq!(key, "agent.name");
            }
            _ => panic!("expected Config Get command"),
        }
    }

    #[test]
    fn cli_parses_config_validate() {
        let cli = Cli::parse_from(["blufio", "config", "validate"]);
        match cli.command {
            Some(Commands::Config {
                action: Some(ConfigCommands::Validate),
            }) => {}
            _ => panic!("expected Config Validate command"),
        }
    }

    #[test]
    fn config_get_agent_name() {
        let config = BlufioConfig::default();
        // Use serde_json traversal approach
        let value = serde_json::to_value(&config).unwrap();
        let agent_name = value.get("agent").unwrap().get("name").unwrap();
        assert_eq!(agent_name, "blufio");
    }

    #[test]
    fn config_get_resolves_known_keys() {
        let config = BlufioConfig::default();
        // Should succeed for known keys
        assert!(cli::config_cmd::cmd_config_get(&config, "agent.name").is_ok());
        assert!(cli::config_cmd::cmd_config_get(&config, "storage.database_path").is_ok());
        assert!(cli::config_cmd::cmd_config_get(&config, "agent.log_level").is_ok());
        assert!(cli::config_cmd::cmd_config_get(&config, "daemon.memory_warn_mb").is_ok());
    }

    #[test]
    fn config_get_fails_for_unknown_key() {
        let config = BlufioConfig::default();
        assert!(cli::config_cmd::cmd_config_get(&config, "nonexistent.key").is_err());
    }

    #[test]
    fn cli_parses_skill_init() {
        let cli = Cli::parse_from(["blufio", "skill", "init", "my-skill"]);
        match cli.command {
            Some(Commands::Skill {
                action: SkillCommands::Init { name },
            }) => {
                assert_eq!(name, "my-skill");
            }
            _ => panic!("expected Skill Init command"),
        }
    }

    #[test]
    fn cli_parses_skill_list() {
        let cli = Cli::parse_from(["blufio", "skill", "list"]);
        match cli.command {
            Some(Commands::Skill {
                action: SkillCommands::List,
            }) => {}
            _ => panic!("expected Skill List command"),
        }
    }

    #[test]
    fn cli_parses_skill_install() {
        let cli = Cli::parse_from([
            "blufio",
            "skill",
            "install",
            "path/to/skill.wasm",
            "path/to/skill.toml",
        ]);
        match cli.command {
            Some(Commands::Skill {
                action:
                    SkillCommands::Install {
                        wasm_path,
                        manifest_path,
                    },
            }) => {
                assert_eq!(wasm_path, "path/to/skill.wasm");
                assert_eq!(manifest_path, "path/to/skill.toml");
            }
            _ => panic!("expected Skill Install command"),
        }
    }

    #[test]
    fn cli_parses_skill_remove() {
        let cli = Cli::parse_from(["blufio", "skill", "remove", "my-skill"]);
        match cli.command {
            Some(Commands::Skill {
                action: SkillCommands::Remove { name },
            }) => {
                assert_eq!(name, "my-skill");
            }
            _ => panic!("expected Skill Remove command"),
        }
    }

    #[test]
    fn cli_parses_plugin_list() {
        let cli = Cli::parse_from(["blufio", "plugin", "list"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::List,
            }) => {}
            _ => panic!("expected Plugin List command"),
        }
    }

    #[test]
    fn cli_parses_plugin_search_with_query() {
        let cli = Cli::parse_from(["blufio", "plugin", "search", "telegram"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Search { query },
            }) => {
                assert_eq!(query, "telegram");
            }
            _ => panic!("expected Plugin Search command"),
        }
    }

    #[test]
    fn cli_parses_plugin_search_no_query() {
        let cli = Cli::parse_from(["blufio", "plugin", "search"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Search { query },
            }) => {
                assert_eq!(query, "");
            }
            _ => panic!("expected Plugin Search command with empty query"),
        }
    }

    #[test]
    fn cli_parses_plugin_install() {
        let cli = Cli::parse_from(["blufio", "plugin", "install", "prometheus"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Install { name },
            }) => {
                assert_eq!(name, "prometheus");
            }
            _ => panic!("expected Plugin Install command"),
        }
    }

    #[test]
    fn cli_parses_plugin_remove() {
        let cli = Cli::parse_from(["blufio", "plugin", "remove", "prometheus"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Remove { name },
            }) => {
                assert_eq!(name, "prometheus");
            }
            _ => panic!("expected Plugin Remove command"),
        }
    }

    #[test]
    fn cli_parses_plugin_update() {
        let cli = Cli::parse_from(["blufio", "plugin", "update"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Update,
            }) => {}
            _ => panic!("expected Plugin Update command"),
        }
    }

    #[test]
    fn cli_parses_mcp_server() {
        let cli = Cli::parse_from(["blufio", "mcp-server"]);
        match cli.command {
            Some(Commands::McpServer) => {}
            _ => panic!("expected McpServer command"),
        }
    }

    #[test]
    fn cli_parses_verify() {
        let cli = Cli::parse_from(["blufio", "verify", "myfile.bin"]);
        match cli.command {
            Some(Commands::Verify { file, signature }) => {
                assert_eq!(file, "myfile.bin");
                assert!(signature.is_none());
            }
            _ => panic!("expected Verify command"),
        }
    }

    #[test]
    fn cli_parses_verify_with_signature() {
        let cli = Cli::parse_from([
            "blufio",
            "verify",
            "myfile.bin",
            "--signature",
            "custom.minisig",
        ]);
        match cli.command {
            Some(Commands::Verify { file, signature }) => {
                assert_eq!(file, "myfile.bin");
                assert_eq!(signature.as_deref(), Some("custom.minisig"));
            }
            _ => panic!("expected Verify command with --signature"),
        }
    }

    #[test]
    fn plugin_config_default_empty_plugins() {
        let config = BlufioConfig::default();
        assert!(config.plugin.plugins.is_empty());
    }

    #[test]
    fn plugin_config_deserializes_from_toml() {
        let toml_str = r#"
[plugin]
plugins = { telegram = true, prometheus = false }
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.plugin.plugins.get("telegram"), Some(&true));
        assert_eq!(config.plugin.plugins.get("prometheus"), Some(&false));
    }

    #[test]
    fn handle_plugin_list_succeeds() {
        let config = BlufioConfig::default();
        let result = cli::plugin_cmd::handle_plugin_command(&config, PluginCommands::List);
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_search_succeeds() {
        let config = BlufioConfig::default();
        let result = cli::plugin_cmd::handle_plugin_command(
            &config,
            PluginCommands::Search {
                query: "telegram".to_string(),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_install_known() {
        let config = BlufioConfig::default();
        let result = cli::plugin_cmd::handle_plugin_command(
            &config,
            PluginCommands::Install {
                name: "prometheus".to_string(),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_install_unknown_fails() {
        let config = BlufioConfig::default();
        let result = cli::plugin_cmd::handle_plugin_command(
            &config,
            PluginCommands::Install {
                name: "nonexistent".to_string(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn handle_plugin_remove_known() {
        let config = BlufioConfig::default();
        let result = cli::plugin_cmd::handle_plugin_command(
            &config,
            PluginCommands::Remove {
                name: "telegram".to_string(),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_update_succeeds() {
        let config = BlufioConfig::default();
        let result = cli::plugin_cmd::handle_plugin_command(&config, PluginCommands::Update);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn set_secret_overwrites_existing() {
        use secrecy::ExposeSecret;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test-overwrite.db");

        let config = BlufioConfig {
            storage: blufio_config::model::StorageConfig {
                database_path: db_path.to_str().unwrap().to_string(),
                ..Default::default()
            },
            vault: blufio_config::model::VaultConfig {
                kdf_memory_cost: 32768,
                kdf_iterations: 2,
                kdf_parallelism: 1,
            },
            ..Default::default()
        };

        unsafe { std::env::set_var("BLUFIO_VAULT_KEY", "test-overwrite") };

        let db = cli::config_cmd::open_db(&config).await.unwrap();
        let conn = db.connection().clone();
        let passphrase = secrecy::SecretString::from("test-overwrite".to_string());
        let vault = blufio_vault::Vault::create(conn, &passphrase, &config.vault)
            .await
            .unwrap();

        // Store initial value.
        vault
            .store_secret("my.key", "original-value")
            .await
            .unwrap();

        // Overwrite with new value.
        vault.store_secret("my.key", "updated-value").await.unwrap();

        // Verify the updated value.
        let retrieved = vault.retrieve_secret("my.key").await.unwrap().unwrap();
        assert_eq!(retrieved.expose_secret(), "updated-value");

        db.close().await.unwrap();

        unsafe { std::env::remove_var("BLUFIO_VAULT_KEY") };
    }

    #[test]
    fn cli_parses_update() {
        let cli = Cli::parse_from(["blufio", "update"]);
        match cli.command {
            Some(Commands::Update { action, yes }) => {
                assert!(action.is_none());
                assert!(!yes);
            }
            _ => panic!("expected Update command"),
        }
    }

    #[test]
    fn cli_parses_update_yes() {
        let cli = Cli::parse_from(["blufio", "update", "--yes"]);
        match cli.command {
            Some(Commands::Update { action, yes }) => {
                assert!(action.is_none());
                assert!(yes);
            }
            _ => panic!("expected Update --yes command"),
        }
    }

    #[test]
    fn cli_parses_update_check() {
        let cli = Cli::parse_from(["blufio", "update", "check"]);
        match cli.command {
            Some(Commands::Update {
                action: Some(UpdateCommands::Check),
                ..
            }) => {}
            _ => panic!("expected Update Check command"),
        }
    }

    #[test]
    fn cli_parses_update_rollback() {
        let cli = Cli::parse_from(["blufio", "update", "rollback"]);
        match cli.command {
            Some(Commands::Update {
                action: Some(UpdateCommands::Rollback),
                ..
            }) => {}
            _ => panic!("expected Update Rollback command"),
        }
    }

    #[test]
    fn cli_parses_healthcheck() {
        let cli = Cli::parse_from(["blufio", "healthcheck"]);
        match cli.command {
            Some(Commands::Healthcheck) => {}
            _ => panic!("expected Healthcheck command"),
        }
    }
}
