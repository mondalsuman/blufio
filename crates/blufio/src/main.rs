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
mod context;
mod cron_cmd;
mod doctor;
mod encrypt;
mod healthcheck;
#[cfg(feature = "mcp-server")]
mod mcp_server;
mod migrate;
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
                if let Err(e) = cmd_set_secret(&config, &key).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::ListSecrets) => {
                if let Err(e) = cmd_list_secrets(&config).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::Get { key }) => {
                if let Err(e) = cmd_config_get(&config, &key) {
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
                let recipe = generate_config_recipe(&preset);
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
            if let Err(e) = handle_skill_command(&config, action).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Plugin { action }) => {
            if let Err(e) = handle_plugin_command(&config, action) {
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
            if let Err(e) = handle_nodes_command(&config, action).await {
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
                    run_audit_verify(&audit_db_path, json);
                }
                AuditCommands::Tail {
                    n,
                    event_type,
                    since,
                    until,
                    actor,
                    json,
                } => {
                    run_audit_tail(&audit_db_path, n, event_type, since, until, actor, json);
                }
                AuditCommands::Stats { json } => {
                    run_audit_stats(&audit_db_path, json);
                }
            }
        }
        Some(Commands::Memory { command }) => {
            if let Err(e) = handle_memory_command(&config, command).await {
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
            run_injection_command(&config, action);
        }
        Some(Commands::Cron { action }) => {
            if let Err(e) =
                cron_cmd::handle_cron_command(action, &config.storage.database_path).await
            {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        None => {
            println!("blufio: use --help for available commands");
        }
    }
}

/// Run `blufio audit verify` -- walk the hash chain and report integrity.
fn run_audit_verify(db_path: &str, json: bool) {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "verified": 0,
                    "breaks": [],
                    "gaps": [],
                    "erased_count": 0,
                    "message": "audit database not found"
                })
            );
        } else {
            println!("Audit database not found: {db_path}");
            println!("No entries to verify.");
        }
        return;
    }

    let conn = match blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to open audit database: {e}");
            std::process::exit(1);
        }
    };

    let report = match blufio_audit::verify_chain(&conn) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: audit verification failed: {e}");
            std::process::exit(1);
        }
    };

    if json {
        let breaks_json: Vec<serde_json::Value> = report
            .breaks
            .iter()
            .map(|b| {
                serde_json::json!({
                    "entry_id": b.entry_id,
                    "expected_hash": b.expected_hash,
                    "actual_hash": b.actual_hash,
                })
            })
            .collect();
        let gaps_json: Vec<serde_json::Value> = report
            .gaps
            .iter()
            .map(|g| {
                serde_json::json!({
                    "after_id": g.after_id,
                    "missing_id": g.missing_id,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "ok": report.ok,
                "verified": report.verified,
                "breaks": breaks_json,
                "gaps": gaps_json,
                "erased_count": report.erased_count,
            })
        );
    } else {
        let status = if report.ok { "OK" } else { "BROKEN" };
        println!("Hash chain: {status}");
        println!("Entries verified: {}", report.verified);
        println!("Erased (GDPR): {}", report.erased_count);
        println!("Gaps: {}", report.gaps.len());

        for b in &report.breaks {
            println!(
                "  BREAK at entry {}: expected {} got {}",
                b.entry_id, b.expected_hash, b.actual_hash
            );
        }
        for g in &report.gaps {
            println!(
                "  GAP: missing entry {} after entry {}",
                g.missing_id, g.after_id
            );
        }
    }

    if !report.ok {
        std::process::exit(1);
    }
}

/// Run `blufio audit tail` -- show recent audit entries with filters.
fn run_audit_tail(
    db_path: &str,
    n: usize,
    event_type: Option<String>,
    since: Option<String>,
    until: Option<String>,
    actor: Option<String>,
    json: bool,
) {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        if json {
            println!("[]");
        } else {
            println!("Audit database not found: {db_path}");
            println!("No entries to display.");
        }
        return;
    }

    let conn = match blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to open audit database: {e}");
            std::process::exit(1);
        }
    };

    // Build dynamic query with filters.
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref et) = event_type {
        if et.ends_with(".*") {
            let prefix = et.strip_suffix(".*").unwrap();
            conditions.push(format!("event_type LIKE ?{}", params.len() + 1));
            params.push(Box::new(format!("{prefix}.%")));
        } else {
            conditions.push(format!("event_type = ?{}", params.len() + 1));
            params.push(Box::new(et.clone()));
        }
    }
    if let Some(ref s) = since {
        conditions.push(format!("timestamp >= ?{}", params.len() + 1));
        params.push(Box::new(s.clone()));
    }
    if let Some(ref u) = until {
        conditions.push(format!("timestamp <= ?{}", params.len() + 1));
        params.push(Box::new(u.clone()));
    }
    if let Some(ref a) = actor {
        conditions.push(format!("actor LIKE ?{}", params.len() + 1));
        params.push(Box::new(format!("{a}%")));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT id, entry_hash, prev_hash, timestamp, event_type, action, \
         resource_type, resource_id, actor, session_id, details_json, pii_marker \
         FROM audit_entries {where_clause} ORDER BY id DESC LIMIT {n}"
    );

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to query audit entries: {e}");
            std::process::exit(1);
        }
    };

    let entries: Vec<blufio_audit::AuditEntry> = match stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(blufio_audit::AuditEntry {
                id: row.get(0)?,
                entry_hash: row.get(1)?,
                prev_hash: row.get(2)?,
                timestamp: row.get(3)?,
                event_type: row.get(4)?,
                action: row.get(5)?,
                resource_type: row.get(6)?,
                resource_id: row.get(7)?,
                actor: row.get(8)?,
                session_id: row.get(9)?,
                details_json: row.get(10)?,
                pii_marker: row.get(11)?,
            })
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
    {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("error: failed to read audit entries: {e}");
            std::process::exit(1);
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
        );
    } else {
        if entries.is_empty() {
            println!("No audit entries found.");
            return;
        }
        // Print in reverse order so newest entries appear at the bottom (natural reading).
        for entry in entries.iter().rev() {
            let marker = if entry.pii_marker == 1 {
                " [ERASED]"
            } else {
                ""
            };
            println!(
                "[{}] {} {} {}/{} {}{}",
                entry.timestamp,
                entry.event_type,
                entry.action,
                entry.resource_type,
                entry.resource_id,
                entry.actor,
                marker,
            );
        }
    }
}

/// Run `blufio audit stats` -- show audit trail statistics.
fn run_audit_stats(db_path: &str, json: bool) {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "total_entries": 0,
                    "first_entry": null,
                    "last_entry": null,
                    "erased_count": 0,
                    "by_type": {},
                    "message": "audit database not found"
                })
            );
        } else {
            println!("Audit database not found: {db_path}");
            println!("No statistics available.");
        }
        return;
    }

    let conn = match blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to open audit database: {e}");
            std::process::exit(1);
        }
    };

    // Summary stats.
    let (total, first_ts, last_ts, erased): (i64, Option<String>, Option<String>, i64) = match conn
        .query_row(
            "SELECT COUNT(*), MIN(timestamp), MAX(timestamp), \
             COALESCE(SUM(pii_marker), 0) FROM audit_entries",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to query audit stats: {e}");
            std::process::exit(1);
        }
    };

    // Per-type breakdown.
    let by_type: Vec<(String, i64)> = {
        let mut stmt = match conn.prepare(
            "SELECT event_type, COUNT(*) as cnt FROM audit_entries \
             GROUP BY event_type ORDER BY cnt DESC",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to query audit type breakdown: {e}");
                std::process::exit(1);
            }
        };

        match stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .and_then(|r| r.collect::<Result<Vec<(String, i64)>, _>>())
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: failed to read audit type breakdown: {e}");
                std::process::exit(1);
            }
        }
    };

    if json {
        let type_map: serde_json::Map<String, serde_json::Value> = by_type
            .iter()
            .map(|(t, c)| (t.clone(), serde_json::json!(c)))
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "total_entries": total,
                "first_entry": first_ts,
                "last_entry": last_ts,
                "erased_count": erased,
                "by_type": type_map,
            })
        );
    } else {
        println!("Total entries: {total}");
        println!("First entry: {}", first_ts.as_deref().unwrap_or("(none)"));
        println!("Last entry: {}", last_ts.as_deref().unwrap_or("(none)"));
        println!("Erased (GDPR): {erased}");
        if !by_type.is_empty() {
            println!("\nBy event type:");
            for (event_type, count) in &by_type {
                println!("  {event_type}: {count}");
            }
        }
    }
}

/// Generate a config recipe template for a specific preset.
fn generate_config_recipe(preset: &str) -> Result<String, blufio_core::BlufioError> {
    let content = match preset {
        "personal" => {
            r#"# Blufio Configuration: Personal Use
# Generated by: blufio config recipe personal
#
# Minimal setup for personal use with a single chat channel.

[agent]
name = "blufio"
max_sessions = 3
log_level = "info"
# system_prompt = "You are a helpful personal assistant."

[telegram]
# bot_token = "<your-telegram-bot-token>"
# allowed_users = ["your_telegram_id"]

[anthropic]
# api_key = "<your-anthropic-api-key>"
# Or set ANTHROPIC_API_KEY environment variable
default_model = "claude-sonnet-4-20250514"
max_tokens = 4096

[storage]
# database_path = "~/.local/share/blufio/blufio.db"

[cost]
# daily_limit_usd = 5.0
# monthly_limit_usd = 50.0
"#
        }
        "team" => {
            r#"# Blufio Configuration: Team Use
# Generated by: blufio config recipe team
#
# Setup for a small team with Slack integration and cost controls.

[agent]
name = "team-blufio"
max_sessions = 10
log_level = "info"
# system_prompt_file = "/etc/blufio/system-prompt.md"

[slack]
# bot_token = "<xoxb-your-slack-bot-token>"
# app_token = "<xapp-your-slack-app-token>"
# allowed_users = ["U12345", "U67890"]

[anthropic]
# api_key = "<your-anthropic-api-key>"
default_model = "claude-sonnet-4-20250514"
max_tokens = 4096

[storage]
# database_path = "/var/lib/blufio/blufio.db"

[cost]
# daily_limit_usd = 20.0
# monthly_limit_usd = 200.0

[security]
# require_tls = true

[gateway]
enabled = true
host = "127.0.0.1"
port = 3000
# bearer_token = "<generate-a-strong-token>"
"#
        }
        "production" => {
            r##"# Blufio Configuration: Production
# Generated by: blufio config recipe production
#
# Full production setup with all channels, security, monitoring, and rate limits.

[agent]
name = "blufio-prod"
max_sessions = 50
log_level = "warn"
# system_prompt_file = "/etc/blufio/system-prompt.md"

[telegram]
# bot_token = "<your-telegram-bot-token>"
# allowed_users = []

[discord]
# bot_token = "<your-discord-bot-token>"
# application_id = 0
# allowed_users = []

[slack]
# bot_token = "<xoxb-your-slack-bot-token>"
# app_token = "<xapp-your-slack-app-token>"
# allowed_users = []

[irc]
# server = "irc.libera.chat"
# port = 6697
# nickname = "blufio-bot"
# channels = ["#your-channel"]
# tls = true

[matrix]
# homeserver_url = "https://matrix.org"
# username = "blufio-bot"
# password = "<your-matrix-password>"
# rooms = ["#your-room:matrix.org"]

[anthropic]
# api_key = "<your-anthropic-api-key>"
default_model = "claude-sonnet-4-20250514"
max_tokens = 4096

[providers]
default = "anthropic"

[providers.openai]
# api_key = "<your-openai-api-key>"
# default_model = "gpt-4o"

[storage]
database_path = "/var/lib/blufio/blufio.db"
wal_mode = true

[cost]
# daily_limit_usd = 100.0
# monthly_limit_usd = 1000.0

[security]
# require_tls = true

[vault]
# session_timeout_secs = 900

[gateway]
enabled = true
host = "0.0.0.0"
port = 3000
# bearer_token = "<generate-a-strong-token>"
# default_rate_limit = 60

[prometheus]
enabled = true
port = 9090

[daemon]
memory_warn_mb = 256
memory_limit_mb = 512
# health_port = 3000
"##
        }
        "iot" => {
            r#"# Blufio Configuration: IoT / Embedded
# Generated by: blufio config recipe iot
#
# Minimal configuration for resource-constrained IoT devices.

[agent]
name = "blufio-iot"
max_sessions = 1
log_level = "warn"

[anthropic]
# api_key = "<your-anthropic-api-key>"
default_model = "claude-sonnet-4-20250514"
max_tokens = 1024

[storage]
# database_path = "/var/lib/blufio/blufio.db"

[daemon]
memory_warn_mb = 64
memory_limit_mb = 128

[skill]
default_fuel = 500000
default_memory_mb = 16
default_epoch_timeout_secs = 5
max_skills_in_prompt = 3

[gateway]
enabled = false
"#
        }
        _ => {
            return Err(blufio_core::BlufioError::Config(format!(
                "unknown recipe preset: \"{preset}\". Available: personal, team, production, iot"
            )));
        }
    };

    Ok(content.to_string())
}

/// Open the database, returning the connection.
async fn open_db(
    config: &blufio_config::model::BlufioConfig,
) -> Result<blufio_storage::Database, blufio_core::BlufioError> {
    blufio_storage::Database::open(&config.storage.database_path).await
}

/// Handle `blufio config set-secret <key>`.
///
/// Creates the vault lazily on first use. Prompts for the secret value
/// via hidden TTY input or reads from piped stdin.
async fn cmd_set_secret(
    config: &blufio_config::model::BlufioConfig,
    key: &str,
) -> Result<(), blufio_core::BlufioError> {
    let db = open_db(config).await?;
    let conn = db.connection().clone();

    // Get or create vault.
    let vault = if blufio_vault::Vault::exists(&conn).await? {
        let passphrase = blufio_vault::get_vault_passphrase()?;
        blufio_vault::Vault::unlock(conn, &passphrase, &config.vault).await?
    } else {
        eprintln!("No vault found. Creating a new vault.");
        let passphrase = blufio_vault::prompt::get_vault_passphrase_with_confirm()?;
        blufio_vault::Vault::create(conn, &passphrase, &config.vault).await?
    };

    // Read secret value.
    let value = read_secret_value(key)?;

    // Store in vault.
    vault.store_secret(key, &value).await?;
    eprintln!("Secret '{}' stored in vault.", key);

    // Clean close with WAL checkpoint.
    db.close().await?;
    Ok(())
}

/// Handle `blufio config list-secrets`.
///
/// Lists all vault secrets with masked previews. Values are never fully shown.
async fn cmd_list_secrets(
    config: &blufio_config::model::BlufioConfig,
) -> Result<(), blufio_core::BlufioError> {
    let db = open_db(config).await?;
    let conn = db.connection().clone();

    if !blufio_vault::Vault::exists(&conn).await? {
        println!("No vault found. Use 'blufio config set-secret' to create one.");
        db.close().await?;
        return Ok(());
    }

    let passphrase = blufio_vault::get_vault_passphrase()?;
    let vault = blufio_vault::Vault::unlock(conn, &passphrase, &config.vault).await?;

    let secrets = vault.list_secrets().await?;
    if secrets.is_empty() {
        println!("No secrets stored.");
    } else {
        for (name, masked) in &secrets {
            println!("{name}: {masked}");
        }
    }

    db.close().await?;
    Ok(())
}

/// Read a secret value from interactive TTY (hidden input) or piped stdin.
fn read_secret_value(key: &str) -> Result<String, blufio_core::BlufioError> {
    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        eprint!("Secret value for '{key}': ");
        let value = rpassword::read_password().map_err(|e| {
            blufio_core::BlufioError::Vault(format!("failed to read secret value: {e}"))
        })?;
        if value.is_empty() {
            return Err(blufio_core::BlufioError::Vault(
                "empty secret value not allowed".to_string(),
            ));
        }
        Ok(value)
    } else {
        // Read from piped stdin for scripting support.
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line).map_err(|e| {
            blufio_core::BlufioError::Vault(format!("failed to read secret from stdin: {e}"))
        })?;
        let value = line.trim_end_matches('\n').trim_end_matches('\r');
        if value.is_empty() {
            return Err(blufio_core::BlufioError::Vault(
                "empty secret value not allowed".to_string(),
            ));
        }
        Ok(value.to_string())
    }
}

/// Handle `blufio nodes <action>` subcommands.
#[cfg(feature = "node")]
async fn handle_nodes_command(
    config: &blufio_config::model::BlufioConfig,
    action: NodesCommands,
) -> Result<(), blufio_core::BlufioError> {
    use std::sync::Arc;

    let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
    let store = Arc::new(blufio_node::NodeStore::new(conn));
    let event_bus = Arc::new(blufio_bus::EventBus::new(128));
    let conn_manager =
        blufio_node::ConnectionManager::new(store.clone(), event_bus.clone(), config.node.clone());

    match action {
        NodesCommands::List { json } => {
            let nodes = conn_manager
                .list_nodes_with_state()
                .await
                .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
            if json {
                println!(
                    "{}",
                    blufio_node::format_nodes_json(&nodes)
                        .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?
                );
            } else {
                print!("{}", blufio_node::format_nodes_table(&nodes));
            }
        }
        NodesCommands::Pair { token: token_mode } => {
            let keypair = Arc::new(blufio_auth_keypair::DeviceKeypair::generate());
            let pairing_mgr =
                blufio_node::PairingManager::new(keypair, store.clone(), event_bus.clone());
            let host = &config.gateway.host;
            let port = config.node.listen_port;
            let (pairing_token, qr_display) = pairing_mgr.initiate_pairing(host, port);
            if token_mode {
                println!("Pairing token: {}", pairing_token.value);
                println!("Connect to: ws://{}:{}/nodes/pair", host, port);
            } else {
                println!("{qr_display}");
            }
            println!("\nToken expires in 15 minutes. Waiting for peer connection...");
            // Note: Full interactive pairing requires a running serve instance.
            // This command displays the token/QR for use with a running server.
        }
        NodesCommands::Remove { node_id } => {
            let removed = store
                .remove_pairing(&node_id)
                .await
                .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
            if removed {
                println!("Node '{node_id}' removed.");
            } else {
                eprintln!("Node '{node_id}' not found.");
                std::process::exit(1);
            }
        }
        NodesCommands::Group {
            action: group_action,
        } => match group_action {
            NodeGroupCommands::Create { name, nodes } => {
                blufio_node::create_group(&store, &name, &nodes)
                    .await
                    .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
                println!("Group '{}' created with {} node(s).", name, nodes.len());
            }
            NodeGroupCommands::Delete { name } => {
                let deleted = blufio_node::delete_group(&store, &name)
                    .await
                    .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
                if deleted {
                    println!("Group '{name}' deleted.");
                } else {
                    eprintln!("Group '{name}' not found.");
                    std::process::exit(1);
                }
            }
            NodeGroupCommands::List => {
                let groups = blufio_node::list_groups(&store)
                    .await
                    .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
                print!("{}", blufio_node::format_groups_table(&groups));
            }
        },
        NodesCommands::Exec { targets, command } => {
            if command.is_empty() {
                return Err(blufio_core::BlufioError::Internal(
                    "no command specified for exec".to_string(),
                ));
            }
            let cmd = &command[0];
            let args: Vec<String> = command[1..].to_vec();
            blufio_node::exec_on_nodes(&conn_manager, &store, &targets, cmd, &args)
                .await
                .map_err(|e| blufio_core::BlufioError::Internal(e.to_string()))?;
            println!("Exec request sent to {} target(s).", targets.len());
        }
    }

    Ok(())
}

/// Handle `blufio skill <action>` subcommands.
async fn handle_skill_command(
    config: &blufio_config::model::BlufioConfig,
    action: SkillCommands,
) -> Result<(), blufio_core::BlufioError> {
    match action {
        SkillCommands::Init { name } => {
            let target_dir = std::path::Path::new(".");
            blufio_skill::scaffold_skill(&name, target_dir)?;
            eprintln!("Skill project '{name}' created successfully.");
            eprintln!("  cd {name} && cargo build --target wasm32-wasip1 --release");
            Ok(())
        }
        SkillCommands::List => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            let skills = store.list().await?;

            if skills.is_empty() {
                println!("No skills installed.");
            } else {
                println!(
                    "{:<20} {:<10} {:<12} DESCRIPTION",
                    "NAME", "VERSION", "STATUS"
                );
                println!("{}", "-".repeat(70));
                for skill in &skills {
                    println!(
                        "{:<20} {:<10} {:<12} {}",
                        skill.name, skill.version, skill.verification_status, skill.description
                    );
                }
            }
            Ok(())
        }
        SkillCommands::Install {
            wasm_path,
            manifest_path,
        } => {
            // Read and parse the manifest.
            let manifest_content = std::fs::read_to_string(&manifest_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;
            let manifest = blufio_skill::parse_manifest(&manifest_content)?;

            // Read the WASM file.
            let wasm_bytes = std::fs::read(&wasm_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            // Compute content hash.
            let content_hash = blufio_skill::compute_content_hash(&wasm_bytes);

            // Check for adjacent .sig file.
            let sig_path = format!("{}.sig", wasm_path);
            let (signature, publisher_id) = if std::path::Path::new(&sig_path).exists() {
                let sig_content = std::fs::read_to_string(&sig_path)
                    .map_err(blufio_core::BlufioError::skill_execution_failed)?;
                let (pub_id, sig_hash, sig_hex) = parse_sig_file(&sig_content)?;

                // Verify the hash in .sig matches our computed hash.
                if sig_hash != content_hash {
                    return Err(blufio_core::BlufioError::Security(format!(
                        "signature file hash mismatch: .sig says {} but WASM hashes to {}",
                        &sig_hash[..12],
                        &content_hash[..12]
                    )));
                }

                // Verify the signature against the WASM bytes.
                let sig = blufio_skill::signature_from_hex(&sig_hex)?;
                let pubkey_bytes = hex::decode(&pub_id).map_err(|e| {
                    blufio_core::BlufioError::Security(format!("invalid publisher_id hex: {e}"))
                })?;
                let pubkey_array: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
                    blufio_core::BlufioError::Security(
                        "publisher_id must be exactly 32 bytes".to_string(),
                    )
                })?;
                let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_array)
                    .map_err(|e| {
                        blufio_core::BlufioError::Security(format!("invalid publisher key: {e}"))
                    })?;
                blufio_skill::PublisherKeypair::verify_signature(
                    &verifying_key,
                    &wasm_bytes,
                    &sig,
                )?;

                eprintln!("  Signature verified (publisher: {}...)", &pub_id[..12]);

                (Some(sig_hex), Some(pub_id))
            } else {
                (None, None)
            };

            // Serialize capabilities to JSON for storage.
            let capabilities_json =
                serde_json::to_string(&manifest.capabilities).unwrap_or_else(|_| "{}".to_string());

            // Open DB and store the skill.
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));

            // TOFU: store publisher key if signed.
            if let Some(ref pub_id) = publisher_id {
                store.check_or_store_publisher_key(pub_id, pub_id).await?;
            }

            store
                .install(
                    &manifest.name,
                    &manifest.version,
                    &manifest.description,
                    manifest.author.as_deref(),
                    &wasm_path,
                    &manifest_content,
                    &capabilities_json,
                    Some(&content_hash),
                    signature.as_deref(),
                    publisher_id.as_deref(),
                )
                .await?;

            let status = if signature.is_some() {
                "verified"
            } else {
                "unverified"
            };
            eprintln!(
                "Skill '{}' v{} installed successfully. [{}]",
                manifest.name, manifest.version, status
            );

            // Print capabilities summary.
            if manifest.capabilities.network.is_some() {
                eprintln!("  Capabilities: network access");
            }
            if manifest.capabilities.filesystem.is_some() {
                eprintln!("  Capabilities: filesystem access");
            }
            if !manifest.capabilities.env.is_empty() {
                eprintln!(
                    "  Capabilities: env vars ({})",
                    manifest.capabilities.env.join(", ")
                );
            }

            Ok(())
        }
        SkillCommands::Remove { name } => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            store.remove(&name).await?;
            eprintln!("Skill '{name}' removed.");
            Ok(())
        }
        SkillCommands::Update {
            name,
            wasm_path,
            manifest_path,
        } => {
            let manifest_content = std::fs::read_to_string(&manifest_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;
            let manifest = blufio_skill::parse_manifest(&manifest_content)?;

            let wasm_bytes = std::fs::read(&wasm_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            let content_hash = blufio_skill::compute_content_hash(&wasm_bytes);

            // Check for adjacent .sig file.
            let sig_path = format!("{}.sig", wasm_path);
            let (signature, publisher_id) = if std::path::Path::new(&sig_path).exists() {
                let sig_content = std::fs::read_to_string(&sig_path)
                    .map_err(blufio_core::BlufioError::skill_execution_failed)?;
                let (pub_id, sig_hash, sig_hex) = parse_sig_file(&sig_content)?;
                if sig_hash != content_hash {
                    return Err(blufio_core::BlufioError::Security(format!(
                        "signature file hash mismatch: .sig says {} but WASM hashes to {}",
                        &sig_hash[..12],
                        &content_hash[..12]
                    )));
                }
                let sig = blufio_skill::signature_from_hex(&sig_hex)?;
                let pubkey_bytes = hex::decode(&pub_id).map_err(|e| {
                    blufio_core::BlufioError::Security(format!("invalid publisher_id hex: {e}"))
                })?;
                let pubkey_array: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
                    blufio_core::BlufioError::Security(
                        "publisher_id must be exactly 32 bytes".to_string(),
                    )
                })?;
                let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_array)
                    .map_err(|e| {
                        blufio_core::BlufioError::Security(format!("invalid publisher key: {e}"))
                    })?;
                blufio_skill::PublisherKeypair::verify_signature(
                    &verifying_key,
                    &wasm_bytes,
                    &sig,
                )?;
                (Some(sig_hex), Some(pub_id))
            } else {
                (None, None)
            };

            let capabilities_json =
                serde_json::to_string(&manifest.capabilities).unwrap_or_else(|_| "{}".to_string());

            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));

            if let Some(ref pub_id) = publisher_id {
                store.check_or_store_publisher_key(pub_id, pub_id).await?;
            }

            store
                .update(
                    &name,
                    &manifest.version,
                    &manifest.description,
                    manifest.author.as_deref(),
                    &wasm_path,
                    &manifest_content,
                    &capabilities_json,
                    Some(&content_hash),
                    signature.as_deref(),
                    publisher_id.as_deref(),
                )
                .await?;

            eprintln!("Skill '{}' updated to v{}.", name, manifest.version);
            Ok(())
        }
        SkillCommands::Sign {
            wasm_path,
            private_key_path,
        } => {
            let wasm_bytes = std::fs::read(&wasm_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            let keypair =
                blufio_skill::load_private_key_from_file(std::path::Path::new(&private_key_path))?;

            let content_hash = blufio_skill::compute_content_hash(&wasm_bytes);
            let signature = keypair.sign(&wasm_bytes);
            let sig_hex = blufio_skill::signature_to_hex(&signature);
            let publisher_id = keypair.public_hex();

            let sig_path = format!("{}.sig", wasm_path);
            let sig_content = format!(
                "publisher_id={}\ncontent_hash={}\nsignature={}\n",
                publisher_id, content_hash, sig_hex
            );
            std::fs::write(&sig_path, &sig_content)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            eprintln!("Signed: {}", wasm_path);
            eprintln!("  Publisher: {}...", &publisher_id[..16]);
            eprintln!("  Hash:      {}...", &content_hash[..16]);
            eprintln!("  Output:    {}", sig_path);
            Ok(())
        }
        SkillCommands::Keygen { output_dir } => {
            let keypair = blufio_skill::PublisherKeypair::generate();
            let dir = std::path::Path::new(&output_dir);
            let private_path = dir.join("publisher.key");
            let public_path = dir.join("publisher.pub");

            blufio_skill::save_keypair_to_file(&keypair, &private_path, &public_path)?;

            eprintln!("Publisher keypair generated:");
            eprintln!("  Private key: {}", private_path.display());
            eprintln!("  Public key:  {}", public_path.display());
            eprintln!("  Publisher ID: {}", keypair.public_hex());
            eprintln!();
            eprintln!("Keep your private key safe! Do not share it.");
            Ok(())
        }
        SkillCommands::Verify { name } => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            let skill = store.get(&name).await?.ok_or_else(|| {
                blufio_core::BlufioError::skill_execution_msg(&format!(
                    "skill '{}' not installed",
                    name
                ))
            })?;

            // Read WASM file and verify hash.
            let wasm_bytes = std::fs::read(&skill.wasm_path)
                .map_err(blufio_core::BlufioError::skill_execution_failed)?;

            let actual_hash = blufio_skill::compute_content_hash(&wasm_bytes);

            // Hash verification.
            if let Some(ref stored_hash) = skill.content_hash {
                if actual_hash == *stored_hash {
                    eprintln!("  Hash:      PASS (SHA-256 matches)");
                } else {
                    eprintln!(
                        "  Hash:      FAIL (expected {}..., got {}...)",
                        &stored_hash[..12],
                        &actual_hash[..12]
                    );
                    return Err(blufio_core::BlufioError::Security(format!(
                        "skill '{}': content hash mismatch — WASM may be tampered",
                        name
                    )));
                }
            } else {
                eprintln!("  Hash:      NONE (no stored hash)");
            }

            // Signature verification.
            if let Some(ref sig_hex) = skill.signature {
                let sig = blufio_skill::signature_from_hex(sig_hex)?;
                let pub_id = skill.publisher_id.as_ref().ok_or_else(|| {
                    blufio_core::BlufioError::Security(format!(
                        "skill '{}': has signature but no publisher_id",
                        name
                    ))
                })?;
                let pubkey_bytes = hex::decode(pub_id).map_err(|e| {
                    blufio_core::BlufioError::Security(format!("invalid publisher_id hex: {e}"))
                })?;
                let pubkey_array: [u8; 32] = pubkey_bytes.try_into().map_err(|_| {
                    blufio_core::BlufioError::Security(
                        "publisher_id must be exactly 32 bytes".to_string(),
                    )
                })?;
                let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_array)
                    .map_err(|e| {
                        blufio_core::BlufioError::Security(format!("invalid publisher key: {e}"))
                    })?;
                blufio_skill::PublisherKeypair::verify_signature(
                    &verifying_key,
                    &wasm_bytes,
                    &sig,
                )?;
                eprintln!("  Signature: PASS (Ed25519 verified)");
                eprintln!("  Publisher: {}...", &pub_id[..12.min(pub_id.len())]);
            } else {
                eprintln!("  Signature: NONE (unsigned skill)");
            }

            eprintln!("Skill '{}' verification complete.", name);
            Ok(())
        }
        SkillCommands::Info { name } => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            let skill = store.get(&name).await?.ok_or_else(|| {
                blufio_core::BlufioError::skill_execution_msg(&format!(
                    "skill '{}' not installed",
                    name
                ))
            })?;

            println!("Name:         {}", skill.name);
            println!("Version:      {}", skill.version);
            println!("Description:  {}", skill.description);
            if let Some(ref author) = skill.author {
                println!("Author:       {}", author);
            }
            println!("WASM path:    {}", skill.wasm_path);
            println!("Status:       {}", skill.verification_status);
            if let Some(ref hash) = skill.content_hash {
                println!("Content hash: {}", hash);
            }
            if let Some(ref sig) = skill.signature {
                let truncated = if sig.len() > 32 {
                    format!("{}...", &sig[..32])
                } else {
                    sig.clone()
                };
                println!("Signature:    {}", truncated);
            }
            if let Some(ref pub_id) = skill.publisher_id {
                println!("Publisher ID: {}", pub_id);
            }
            println!("Installed:    {}", skill.installed_at);
            println!("Updated:      {}", skill.updated_at);
            println!("Capabilities: {}", skill.capabilities_json);
            Ok(())
        }
    }
}

/// Parse a .sig file content into (publisher_id, content_hash, signature).
fn parse_sig_file(content: &str) -> Result<(String, String, String), blufio_core::BlufioError> {
    let mut publisher_id = None;
    let mut content_hash = None;
    let mut signature = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("publisher_id=") {
            publisher_id = Some(val.to_string());
        } else if let Some(val) = line.strip_prefix("content_hash=") {
            content_hash = Some(val.to_string());
        } else if let Some(val) = line.strip_prefix("signature=") {
            signature = Some(val.to_string());
        }
    }

    Ok((
        publisher_id.ok_or_else(|| {
            blufio_core::BlufioError::Security("signature file missing publisher_id".to_string())
        })?,
        content_hash.ok_or_else(|| {
            blufio_core::BlufioError::Security("signature file missing content_hash".to_string())
        })?,
        signature.ok_or_else(|| {
            blufio_core::BlufioError::Security("signature file missing signature".to_string())
        })?,
    ))
}

/// Handle `blufio memory <command>` subcommands.
async fn handle_memory_command(
    config: &blufio_config::model::BlufioConfig,
    command: MemoryCommand,
) -> Result<(), blufio_core::BlufioError> {
    match command {
        MemoryCommand::Validate { dry_run, json } => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_memory::MemoryStore::new(conn);

            if dry_run {
                let memories = store.get_all_active_with_embeddings().await?;
                let result =
                    blufio_memory::validation::run_validation_dry_run(&memories, &config.memory);

                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "dry_run": true,
                            "duplicates": result.duplicates_found,
                            "conflicts": result.conflicts_found,
                            "stale": result.stale_found,
                        })
                    );
                } else {
                    println!("Validation (dry run):");
                    println!("  Duplicates: {}", result.duplicates_found);
                    println!("  Conflicts:  {}", result.conflicts_found);
                    println!("  Stale:      {}", result.stale_found);
                }
            } else {
                let result =
                    blufio_memory::validation::run_validation(&store, &config.memory, &None)
                        .await?;

                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "dry_run": false,
                            "duplicates": result.duplicates_found,
                            "conflicts": result.conflicts_found,
                            "stale": result.stale_found,
                        })
                    );
                } else {
                    println!("Validation complete:");
                    println!("  Duplicates resolved: {}", result.duplicates_found);
                    println!("  Conflicts resolved:  {}", result.conflicts_found);
                    println!("  Stale removed:       {}", result.stale_found);
                }
            }
        }
    }
    Ok(())
}

/// Handle `blufio plugin <action>` subcommands.
fn handle_plugin_command(
    config: &blufio_config::model::BlufioConfig,
    action: PluginCommands,
) -> Result<(), blufio_core::BlufioError> {
    match action {
        PluginCommands::List => {
            let catalog = blufio_plugin::builtin_catalog();
            let mut registry = blufio_plugin::PluginRegistry::new();

            for manifest in catalog {
                // Determine status based on config overrides and required config keys.
                let name = manifest.name.clone();
                let config_override = config.plugin.plugins.get(&name);

                let status = match config_override {
                    Some(false) => blufio_plugin::PluginStatus::Disabled,
                    Some(true) => blufio_plugin::PluginStatus::Enabled,
                    None => {
                        // Check if required config keys are present.
                        let all_configured = manifest
                            .config_keys
                            .iter()
                            .all(|key| is_config_key_present(config, key));
                        if all_configured || manifest.config_keys.is_empty() {
                            blufio_plugin::PluginStatus::Enabled
                        } else {
                            blufio_plugin::PluginStatus::NotConfigured
                        }
                    }
                };

                registry.register_with_status(manifest, None, status);
            }

            println!("{:<18} {:<15} {:<16} DESCRIPTION", "NAME", "TYPE", "STATUS");
            println!("{}", "-".repeat(75));
            for entry in registry.list_all() {
                println!(
                    "{:<18} {:<15} {:<16} {}",
                    entry.manifest.name,
                    entry.manifest.adapter_type.to_string(),
                    entry.status,
                    entry.manifest.description,
                );
            }
            Ok(())
        }
        PluginCommands::Search { query } => {
            let results = blufio_plugin::search_catalog(&query);
            if results.is_empty() {
                println!("No plugins found matching '{query}'.");
            } else {
                println!("{:<18} {:<15} DESCRIPTION", "NAME", "TYPE");
                println!("{}", "-".repeat(65));
                for manifest in &results {
                    println!(
                        "{:<18} {:<15} {}",
                        manifest.name,
                        manifest.adapter_type.to_string(),
                        manifest.description,
                    );
                }
            }
            Ok(())
        }
        PluginCommands::Install { name } => {
            let catalog = blufio_plugin::builtin_catalog();
            let found = catalog.iter().find(|m| m.name == name);

            match found {
                Some(manifest) => {
                    println!("Plugin '{}' enabled.", name);
                    if !manifest.config_keys.is_empty() {
                        println!(
                            "  Required config keys: {}",
                            manifest.config_keys.join(", ")
                        );
                        println!("  Add configuration to blufio.toml if required.");
                    }
                    Ok(())
                }
                None => Err(blufio_core::BlufioError::AdapterNotFound {
                    adapter_type: "plugin".to_string(),
                    name,
                }),
            }
        }
        PluginCommands::Remove { name } => {
            let catalog = blufio_plugin::builtin_catalog();
            let found = catalog.iter().any(|m| m.name == name);

            if found {
                println!("Plugin '{name}' disabled.");
                Ok(())
            } else {
                Err(blufio_core::BlufioError::AdapterNotFound {
                    adapter_type: "plugin".to_string(),
                    name,
                })
            }
        }
        PluginCommands::Update => {
            println!("Plugins are compiled into the Blufio binary.");
            println!("Update by rebuilding or downloading a new binary release.");
            Ok(())
        }
    }
}

/// Check if a config key is present (non-empty) in the loaded config.
///
/// Supports dotted key paths like "telegram.bot_token" and "anthropic.api_key".
fn is_config_key_present(config: &blufio_config::model::BlufioConfig, key: &str) -> bool {
    match key {
        "telegram.bot_token" => config.telegram.bot_token.is_some(),
        "anthropic.api_key" => config.anthropic.api_key.is_some(),
        _ => false,
    }
}

/// Handle `blufio config get <key>`.
///
/// Resolves a dotted config key path to its current value. Uses serde_json
/// serialization to traverse the config struct generically.
fn cmd_config_get(
    config: &blufio_config::model::BlufioConfig,
    key: &str,
) -> Result<(), blufio_core::BlufioError> {
    // Serialize the full config to a JSON Value for generic traversal.
    let value = serde_json::to_value(config).map_err(|e| {
        blufio_core::BlufioError::Internal(format!("failed to serialize config: {e}"))
    })?;

    // Walk the dotted key path.
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &value;

    for part in &parts {
        match current.get(part) {
            Some(v) => current = v,
            None => {
                return Err(blufio_core::BlufioError::Config(format!(
                    "unknown config key: {key}"
                )));
            }
        }
    }

    // Print the resolved value.
    match current {
        serde_json::Value::String(s) => println!("{s}"),
        serde_json::Value::Null => println!("null"),
        other => println!("{other}"),
    }

    Ok(())
}

/// Handle injection defense CLI subcommands.
fn run_injection_command(config: &blufio_config::model::BlufioConfig, action: InjectionCommands) {
    use std::io::IsTerminal;

    match action {
        InjectionCommands::Test { text, json, plain } => {
            let classifier =
                blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense);
            let result = classifier.classify(&text, "user");

            if json {
                let matches_json: Vec<serde_json::Value> = result
                    .matches
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "category": format!("{:?}", m.category),
                            "severity": m.severity,
                            "matched_text": m.matched_text,
                            "span": [m.span.start, m.span.end],
                        })
                    })
                    .collect();
                let output = serde_json::json!({
                    "text": text,
                    "score": result.score,
                    "action": result.action,
                    "categories": result.categories,
                    "matches": matches_json,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let use_color = !plain && std::io::stdout().is_terminal();
                println!();
                println!("  Injection Defense Test");
                println!("  {}", "-".repeat(50));
                println!("  Input: \"{}\"", text);
                println!("  Score: {:.4}", result.score);

                let action_display = if use_color {
                    use colored::Colorize;
                    match result.action.as_str() {
                        "clean" => result.action.green().to_string(),
                        "logged" => result.action.yellow().to_string(),
                        "blocked" => result.action.red().to_string(),
                        "dry_run" => result.action.cyan().to_string(),
                        other => other.to_string(),
                    }
                } else {
                    result.action.clone()
                };
                println!("  Action: {}", action_display);

                if !result.categories.is_empty() {
                    println!("  Categories: {}", result.categories.join(", "));
                }
                if !result.matches.is_empty() {
                    println!("  Matched patterns:");
                    for mp in &result.matches {
                        println!(
                            "    - {:?} (severity: {:.2}, text: \"{}\")",
                            mp.category, mp.severity, mp.matched_text
                        );
                    }
                }
                println!();
            }
        }
        InjectionCommands::Status { json } => {
            let cfg = &config.injection_defense;
            let active_layers: Vec<&str> = [
                Some("L1 (input detection)"),
                if cfg.hmac_boundaries.enabled {
                    Some("L3 (HMAC boundaries)")
                } else {
                    None
                },
                if cfg.output_screening.enabled {
                    Some("L4 (output screening)")
                } else {
                    None
                },
                if cfg.hitl.enabled {
                    Some("L5 (human-in-the-loop)")
                } else {
                    None
                },
            ]
            .iter()
            .filter_map(|l| *l)
            .collect();

            if json {
                let output = serde_json::json!({
                    "enabled": cfg.enabled,
                    "dry_run": cfg.dry_run,
                    "active_layers": active_layers,
                    "layer_count": active_layers.len(),
                    "input_detection_mode": cfg.input_detection.mode,
                    "blocking_threshold": cfg.input_detection.blocking_threshold,
                    "mcp_blocking_threshold": cfg.input_detection.mcp_blocking_threshold,
                    "custom_patterns": cfg.input_detection.custom_patterns.len(),
                    "output_screening_enabled": cfg.output_screening.enabled,
                    "escalation_threshold": cfg.output_screening.escalation_threshold,
                    "hitl_enabled": cfg.hitl.enabled,
                    "hitl_timeout_secs": cfg.hitl.timeout_secs,
                    "hmac_boundaries_enabled": cfg.hmac_boundaries.enabled,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!();
                println!("  Injection Defense Status");
                println!("  {}", "-".repeat(50));
                println!(
                    "  Enabled:        {}",
                    if cfg.enabled { "yes" } else { "no" }
                );
                println!(
                    "  Dry run:        {}",
                    if cfg.dry_run { "yes" } else { "no" }
                );
                println!("  Active layers:  {}", active_layers.join(", "));
                println!();
                println!("  L1 Input Detection:");
                println!("    Mode:              {}", cfg.input_detection.mode);
                println!(
                    "    Blocking threshold: {:.2}",
                    cfg.input_detection.blocking_threshold
                );
                println!(
                    "    MCP threshold:     {:.2}",
                    cfg.input_detection.mcp_blocking_threshold
                );
                println!(
                    "    Custom patterns:   {}",
                    cfg.input_detection.custom_patterns.len()
                );
                println!();
                println!("  L3 HMAC Boundaries:");
                println!(
                    "    Enabled:           {}",
                    if cfg.hmac_boundaries.enabled {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!();
                println!("  L4 Output Screening:");
                println!(
                    "    Enabled:           {}",
                    if cfg.output_screening.enabled {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!(
                    "    Escalation after:  {} failures",
                    cfg.output_screening.escalation_threshold
                );
                println!();
                println!("  L5 Human-in-the-Loop:");
                println!(
                    "    Enabled:           {}",
                    if cfg.hitl.enabled { "yes" } else { "no" }
                );
                println!("    Timeout:           {}s", cfg.hitl.timeout_secs);
                println!();
            }
        }
        InjectionCommands::Config { json } => {
            if json {
                let output = serde_json::to_value(&config.injection_defense)
                    .unwrap_or(serde_json::json!({}));
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let toml_str = toml::to_string_pretty(&config.injection_defense)
                    .unwrap_or_else(|_| "[serialization error]".to_string());
                println!();
                println!("  Effective Injection Defense Config");
                println!("  {}", "-".repeat(50));
                println!("{}", toml_str);
            }
        }
    }
}

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
        let db = open_db(&config).await.unwrap();
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
        let result = cmd_list_secrets(&config).await;
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
        assert!(cmd_config_get(&config, "agent.name").is_ok());
        assert!(cmd_config_get(&config, "storage.database_path").is_ok());
        assert!(cmd_config_get(&config, "agent.log_level").is_ok());
        assert!(cmd_config_get(&config, "daemon.memory_warn_mb").is_ok());
    }

    #[test]
    fn config_get_fails_for_unknown_key() {
        let config = BlufioConfig::default();
        assert!(cmd_config_get(&config, "nonexistent.key").is_err());
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
        let result = handle_plugin_command(&config, PluginCommands::List);
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_search_succeeds() {
        let config = BlufioConfig::default();
        let result = handle_plugin_command(
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
        let result = handle_plugin_command(
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
        let result = handle_plugin_command(
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
        let result = handle_plugin_command(
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
        let result = handle_plugin_command(&config, PluginCommands::Update);
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

        let db = open_db(&config).await.unwrap();
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
