use std::path::Path;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "myground", version, about = "Self-hosting platform — hold your ground")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Data directory (default: ~/.myground)
    #[arg(long, global = true)]
    data_dir: Option<String>,

    /// Admin username for authenticating CLI commands
    #[arg(long, global = true)]
    username: Option<String>,

    /// Admin password for authenticating CLI commands
    #[arg(long, global = true)]
    password: Option<String>,

    /// API key for authenticating CLI commands (or set MYGROUND_API_KEY env var)
    #[arg(long, global = true)]
    api_key: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MyGround server
    Start {
        /// Port to listen on
        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        /// Address to bind to
        #[arg(short, long, default_value = "0.0.0.0")]
        address: String,

        /// Tailscale auth key (enables Tailscale on start)
        #[arg(long)]
        tailscale_key: Option<String>,
    },
    /// Show status of MyGround and managed services
    Status,
    /// Authenticate CLI session
    Login,
    /// Remove CLI session
    Logout,
    /// Manage services
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Disk information and health
    Disk {
        #[command(subcommand)]
        action: DiskAction,
    },
    /// Backup management with Restic
    Backup {
        #[command(subcommand)]
        action: BackupAction,
    },
    /// Tailscale networking
    Tailscale {
        #[command(subcommand)]
        action: TailscaleAction,
    },
    /// Destroy everything: stop all containers, remove all data, clean slate
    Nuke,
}

#[derive(Subcommand)]
enum TailscaleAction {
    /// Show Tailscale status
    Status,
    /// Enable Tailscale with an auth key
    Enable {
        /// Tailscale auth key
        auth_key: String,
    },
    /// Disable Tailscale
    Disable,
}

#[derive(Subcommand)]
enum ServiceAction {
    /// List all services and their status
    List,
    /// Install a service
    Install {
        /// Service ID (e.g., whoami, filebrowser, immich)
        id: String,
    },
    /// Start a service
    Start {
        /// Service ID
        id: String,
    },
    /// Stop a service
    Stop {
        /// Service ID
        id: String,
    },
    /// Remove a service and its data
    Remove {
        /// Service ID
        id: String,
    },
}

#[derive(Subcommand)]
enum DiskAction {
    /// List all mounted disks with space info
    List,
    /// Show SMART health for all disks
    Health,
}

#[derive(Subcommand)]
enum BackupAction {
    /// Pull restic image and initialize backup repository
    Init,
    /// Run backup for all installed services (or one specific service)
    Run {
        /// Service ID to backup (omit for all)
        service: Option<String>,
    },
    /// List backup snapshots
    Snapshots,
    /// Restore a snapshot
    Restore {
        /// Snapshot ID to restore
        snapshot_id: String,
        /// Target path for restore (default: temp dir under data dir)
        #[arg(short, long)]
        target: Option<String>,
    },
    /// Configure backup repository
    Configure {
        /// Repository path or s3:url
        repository: String,
        /// Repository password
        password: String,
    },
}

fn create_state(data_dir_override: Option<&str>) -> myground::AppState {
    let data_dir = match data_dir_override {
        Some(path) => std::path::PathBuf::from(path),
        None => myground::config::data_dir(),
    };
    myground::config::ensure_data_dir(&data_dir).expect("Failed to create data directory");
    myground::AppState::new(data_dir)
}

// ── CLI Authentication ──────────────────────────────────────────────────────

/// Path to the CLI session token file.
fn cli_session_path(state: &myground::AppState) -> std::path::PathBuf {
    state.data_dir.join(".cli-session")
}

/// Verify CLI credentials against stored auth config.
/// Checks (in order): --api-key flag/env, --username/--password flags, stored session file.
fn require_cli_auth(
    state: &myground::AppState,
    username: Option<&str>,
    password: Option<&str>,
    api_key: Option<&str>,
) {
    let auth_config = match myground::config::load_auth_config(&state.data_dir) {
        Ok(Some(c)) => c,
        Ok(None) => return, // No auth configured yet — allow CLI access
        Err(e) => fatal(format!("Failed to load auth config: {e}")),
    };

    // 1. Check --api-key flag or MYGROUND_API_KEY env var
    if let Some(key) = api_key {
        if auth_config
            .api_keys
            .iter()
            .any(|entry| myground::auth::verify_password(key, &entry.key_hash))
        {
            return;
        }
        fatal("Invalid API key");
    }

    // 2. Check --username/--password flags
    if let (Some(user), Some(pass)) = (username, password) {
        if user == auth_config.username
            && myground::auth::verify_password(pass, &auth_config.password_hash)
        {
            return;
        }
        fatal("Invalid credentials");
    }

    // 3. Check stored CLI session token against hash in config
    if let Some(ref token_hash) = auth_config.cli_token_hash {
        if let Ok(stored_token) = std::fs::read_to_string(cli_session_path(state)) {
            if myground::auth::verify_password(stored_token.trim(), token_hash) {
                return;
            }
        }
    }

    // 4. No valid auth found
    fatal("Authentication required. Run 'myground login' or pass --username/--password/--api-key flags.");
}

/// Interactive login: prompt for credentials and save session.
fn cmd_login(state: &myground::AppState) {
    let auth_config = match myground::config::load_auth_config(&state.data_dir) {
        Ok(Some(c)) => c,
        Ok(None) => {
            println!("No auth configured. Start the server first to set up.");
            return;
        }
        Err(e) => fatal(format!("Failed to load auth config: {e}")),
    };

    // Read username
    eprint!("Username: ");
    let mut username = String::new();
    std::io::stdin().read_line(&mut username).unwrap_or_default();
    let username = username.trim();

    // Read password (from stdin, no echo control needed for CLI)
    eprint!("Password: ");
    let mut password = String::new();
    std::io::stdin().read_line(&mut password).unwrap_or_default();
    let password = password.trim();

    if username == auth_config.username
        && myground::auth::verify_password(password, &auth_config.password_hash)
    {
        // Generate a cryptographic token, store raw token in file and hash in config
        let token = myground::auth::generate_session_token();
        let token_hash = myground::auth::hash_password(&token).expect("Failed to hash token");
        if let Err(e) = std::fs::write(cli_session_path(state), &token) {
            fatal(format!("Failed to save session: {e}"));
        }
        // Restrict file permissions to owner-only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                cli_session_path(state),
                std::fs::Permissions::from_mode(0o600),
            );
        }
        // Store token hash in auth config
        let mut auth_cfg = auth_config;
        auth_cfg.cli_token_hash = Some(token_hash);
        myground::config::save_auth_config(&state.data_dir, &auth_cfg)
            .expect("Failed to save auth config");
        println!("Logged in.");
    } else {
        fatal("Invalid credentials");
    }
}

fn cmd_logout(state: &myground::AppState) {
    let path = cli_session_path(state);
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    // Clear token hash from config
    if let Ok(Some(mut auth_cfg)) = myground::config::load_auth_config(&state.data_dir) {
        if auth_cfg.cli_token_hash.is_some() {
            auth_cfg.cli_token_hash = None;
            let _ = myground::config::save_auth_config(&state.data_dir, &auth_cfg);
        }
    }
    println!("Logged out.");
}

/// Load backup config or exit with an error message.
fn require_backup_config(base: &Path) -> myground::config::BackupConfig {
    match myground::config::load_backup_config(base) {
        Ok(Some(c)) => c,
        Ok(None) => {
            eprintln!("No backup config set. Run: myground backup configure <repo> <password>");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to load backup config: {e}");
            std::process::exit(1);
        }
    }
}

/// Exit with an error message.
fn fatal(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "myground=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cli_data_dir = cli.data_dir.as_deref();
    let cli_user = cli.username.as_deref();
    let cli_pass = cli.password.as_deref();
    let cli_api_key = cli
        .api_key
        .or_else(|| std::env::var("MYGROUND_API_KEY").ok());

    match cli.command {
        Some(Commands::Start {
            port,
            address,
            tailscale_key,
        }) => {
            let state = create_state(cli_data_dir);
            setup_from_cli(&state, cli_user, cli_pass, tailscale_key.as_deref()).await;
            myground::serve(state, &address, port).await;
        }
        Some(Commands::Status) => {
            let state = create_state(cli_data_dir);
            cmd_status(&state).await;
        }
        Some(Commands::Login) => {
            let state = create_state(cli_data_dir);
            cmd_login(&state);
        }
        Some(Commands::Logout) => {
            let state = create_state(cli_data_dir);
            cmd_logout(&state);
        }
        Some(Commands::Service { action }) => {
            let state = create_state(cli_data_dir);
            match action {
                ServiceAction::List => cmd_service_list(&state).await,
                ServiceAction::Install { id } => {
                    require_cli_auth(&state, cli_user, cli_pass, cli_api_key.as_deref());
                    cmd_service_install(&state, &id).await;
                }
                ServiceAction::Start { id } => {
                    require_cli_auth(&state, cli_user, cli_pass, cli_api_key.as_deref());
                    run_service_action(&id, "start", myground::services::start_service(&state.data_dir, &id)).await;
                }
                ServiceAction::Stop { id } => {
                    require_cli_auth(&state, cli_user, cli_pass, cli_api_key.as_deref());
                    run_service_action(&id, "stop", myground::services::stop_service(&state.data_dir, &id)).await;
                }
                ServiceAction::Remove { id } => {
                    require_cli_auth(&state, cli_user, cli_pass, cli_api_key.as_deref());
                    run_service_action(&id, "remove", myground::services::remove_service(&state.data_dir, &id)).await;
                }
            }
        }
        Some(Commands::Disk { action }) => match action {
            DiskAction::List => cmd_disk_list(),
            DiskAction::Health => cmd_disk_health(),
        },
        Some(Commands::Backup { action }) => {
            let state = create_state(cli_data_dir);
            require_cli_auth(&state, cli_user, cli_pass, cli_api_key.as_deref());
            match action {
                BackupAction::Init => cmd_backup_init(&state).await,
                BackupAction::Run { service } => cmd_backup_run(&state, service.as_deref()).await,
                BackupAction::Snapshots => cmd_backup_snapshots(&state).await,
                BackupAction::Restore { snapshot_id, target } => {
                    cmd_backup_restore(&state, &snapshot_id, target).await;
                }
                BackupAction::Configure { repository, password } => {
                    cmd_backup_configure(&state, &repository, &password);
                }
            }
        }
        Some(Commands::Tailscale { action }) => {
            let state = create_state(cli_data_dir);
            match action {
                TailscaleAction::Status => cmd_tailscale_status(&state).await,
                TailscaleAction::Enable { auth_key } => {
                    require_cli_auth(&state, cli_user, cli_pass, cli_api_key.as_deref());
                    cmd_tailscale_enable(&state, &auth_key).await;
                }
                TailscaleAction::Disable => {
                    require_cli_auth(&state, cli_user, cli_pass, cli_api_key.as_deref());
                    cmd_tailscale_disable(&state).await;
                }
            }
        }
        Some(Commands::Nuke) => {
            let state = create_state(cli_data_dir);
            let data_dir = &state.data_dir;
            println!("NUKING MyGround — stopping all containers, deleting all data...");
            let actions = myground::services::nuke_all(data_dir).await;
            for action in &actions {
                println!("  {action}");
            }
            if actions.is_empty() {
                println!("Nothing to clean up.");
            } else {
                println!("Done. Clean slate.");
            }
        }
        None => {
            let state = create_state(cli_data_dir);
            setup_from_cli(&state, cli_user, cli_pass, None).await;
            myground::serve(state, "0.0.0.0", 8080).await;
        }
    }
}

// ── Status ──────────────────────────────────────────────────────────────────

async fn cmd_status(state: &myground::AppState) {
    println!("MyGround v{}", env!("CARGO_PKG_VERSION"));

    let docker_status = myground::docker::get_status(&state.docker).await;
    if docker_status.connected {
        println!(
            "Docker: connected (v{})",
            docker_status.version.as_deref().unwrap_or("unknown")
        );
    } else {
        println!("Docker: not connected");
    }

    let installed = myground::config::list_installed_services(&state.data_dir);
    if installed.is_empty() {
        println!("Services: none installed");
    } else {
        println!("Installed services: {}", installed.join(", "));
    }

    let disks = myground::disk::list_disks();
    let total: u64 = disks.iter().map(|d| d.total_bytes).sum();
    let available: u64 = disks.iter().map(|d| d.available_bytes).sum();
    println!(
        "Disks: {} mounted, {} total, {} available",
        disks.len(),
        format_bytes(total),
        format_bytes(available)
    );
}

// ── Service commands ────────────────────────────────────────────────────────

async fn cmd_service_list(state: &myground::AppState) {
    let installed = myground::config::list_installed_services(&state.data_dir);
    let container_map = myground::docker::get_container_statuses(&state.docker, &installed).await;

    println!("{:<15} {:<20} {:<12} {:<10}", "ID", "NAME", "INSTALLED", "STATUS");
    println!("{}", "-".repeat(57));

    let mut services: Vec<_> = state.registry.iter().collect();
    services.sort_by_key(|(id, _)| (*id).clone());

    for (id, def) in services {
        let is_installed = installed.contains(id);
        let status = if let Some(containers) = container_map.get(id.as_str()) {
            containers.first().map(|c| c.state.clone()).unwrap_or_else(|| "unknown".to_string())
        } else if is_installed {
            "stopped".to_string()
        } else {
            "-".to_string()
        };

        println!(
            "{:<15} {:<20} {:<12} {:<10}",
            id,
            def.metadata.name,
            if is_installed { "yes" } else { "no" },
            status
        );
    }
}

async fn cmd_service_install(state: &myground::AppState, id: &str) {
    if !state.registry.contains_key(id) {
        eprintln!("Unknown service: {id}");
        eprintln!(
            "Available: {}",
            state.registry.keys().cloned().collect::<Vec<_>>().join(", ")
        );
        std::process::exit(1);
    }

    println!("Installing {id}...");
    match myground::services::install_service(&state.data_dir, &state.registry, id, None, None).await {
        Ok(result) => println!("Service {} installed on port {}.", result.instance_id, result.port),
        Err(e) => fatal(format!("Failed to install {id}: {e}")),
    }
}

async fn run_service_action(
    id: &str,
    verb: &str,
    action: impl std::future::Future<Output = Result<(), myground::error::ServiceError>>,
) {
    let verb_ing = match verb {
        "stop" => "Stopping",
        "remove" => "Removing",
        _ => "Starting",
    };
    println!("{verb_ing} {id}...");
    match action.await {
        Ok(()) => println!("Service {id} {verb}ed."),
        Err(e) => fatal(format!("Failed to {verb} {id}: {e}")),
    }
}

// ── Backup commands ─────────────────────────────────────────────────────────

async fn cmd_backup_init(state: &myground::AppState) {
    let config = require_backup_config(&state.data_dir);

    println!("Pulling restic image...");
    if let Err(e) = myground::backup::ensure_restic_image().await {
        fatal(format!("Failed to pull restic image: {e}"));
    }

    println!("Initializing repository...");
    match myground::backup::init_repo(&config).await {
        Ok(msg) => println!("{msg}"),
        Err(e) => fatal(format!("Failed to init repo: {e}")),
    }
}

async fn cmd_backup_run(state: &myground::AppState, service: Option<&str>) {
    let backup_config = require_backup_config(&state.data_dir);
    let global_config = myground::config::load_global_config(&state.data_dir).unwrap_or_default();

    let results = if let Some(id) = service {
        println!("Backing up {id}...");
        myground::backup::backup_service(
            &state.data_dir, id, &state.registry, &global_config, &backup_config,
        )
        .await
    } else {
        println!("Backing up all installed services...");
        myground::backup::backup_all(
            &state.data_dir, &state.registry, &global_config, &backup_config,
        )
        .await
    };

    match results {
        Ok(results) => {
            for r in &results {
                println!("  Snapshot: {} ({} new files, {} bytes)", r.snapshot_id, r.files_new, r.bytes_added);
            }
            println!("Backup complete: {} snapshot(s)", results.len());
        }
        Err(e) => fatal(format!("Backup failed: {e}")),
    }
}

async fn cmd_backup_snapshots(state: &myground::AppState) {
    let config = require_backup_config(&state.data_dir);

    match myground::backup::list_snapshots(&config).await {
        Ok(snapshots) if snapshots.is_empty() => println!("No snapshots found."),
        Ok(snapshots) => {
            println!("{:<12} {:<25} {:<20} {}", "ID", "TIME", "TAGS", "PATHS");
            println!("{}", "-".repeat(70));
            for s in &snapshots {
                println!(
                    "{:<12} {:<25} {:<20} {}",
                    &s.id[..8.min(s.id.len())],
                    s.time,
                    s.tags.join(","),
                    s.paths.join(","),
                );
            }
        }
        Err(e) => fatal(format!("Failed to list snapshots: {e}")),
    }
}

async fn cmd_backup_restore(state: &myground::AppState, snapshot_id: &str, target: Option<String>) {
    let config = require_backup_config(&state.data_dir);
    let target_path = target.unwrap_or_else(|| {
        state.data_dir.join("restores").join(snapshot_id).to_string_lossy().to_string()
    });

    println!("Restoring {snapshot_id} to {target_path}...");
    match myground::backup::restore_snapshot(&target_path, snapshot_id, &config).await {
        Ok(_) => println!("Restore complete."),
        Err(e) => fatal(format!("Restore failed: {e}")),
    }
}

fn cmd_backup_configure(state: &myground::AppState, repository: &str, password: &str) {
    let config = myground::config::BackupConfig {
        repository: Some(repository.to_string()),
        password: Some(password.to_string()),
        ..Default::default()
    };
    match myground::config::save_backup_config(&state.data_dir, &config) {
        Ok(()) => println!("Backup config saved."),
        Err(e) => fatal(format!("Failed to save config: {e}")),
    }
}

// ── Disk commands ───────────────────────────────────────────────────────────

fn cmd_disk_list() {
    let disks = myground::disk::list_disks();
    if disks.is_empty() {
        println!("No disks found.");
        return;
    }

    println!(
        "{:<20} {:<8} {:>10} {:>10} {:>10} {:>5}",
        "MOUNT", "FS", "TOTAL", "USED", "AVAIL", "USE%"
    );
    println!("{}", "-".repeat(68));

    for d in &disks {
        let pct = if d.total_bytes > 0 {
            (d.used_bytes as f64 / d.total_bytes as f64 * 100.0) as u64
        } else {
            0
        };
        println!(
            "{:<20} {:<8} {:>10} {:>10} {:>10} {:>4}%",
            d.mount_point,
            d.fs_type,
            format_bytes(d.total_bytes),
            format_bytes(d.used_bytes),
            format_bytes(d.available_bytes),
            pct
        );
    }
}

fn cmd_disk_health() {
    let health = myground::disk::smart_health_all();
    if health.is_empty() {
        println!("SMART health: smartctl not available or no devices found.");
        println!("Install smartmontools for disk health monitoring.");
        return;
    }

    for h in &health {
        let status = if h.healthy { "HEALTHY" } else { "FAILING" };
        print!("{}: {}", h.device, status);
        if let Some(temp) = h.temperature_celsius {
            print!(", {}°C", temp);
        }
        if let Some(hours) = h.power_on_hours {
            print!(", {} hours on", hours);
        }
        println!();
    }
}

// ── CLI setup (bypasses web setup wizard) ────────────────────────────────

async fn setup_from_cli(
    state: &myground::AppState,
    username: Option<&str>,
    password: Option<&str>,
    tailscale_key: Option<&str>,
) {
    // Set up auth from CLI flags if both username and password are provided
    if let (Some(user), Some(pass)) = (username, password) {
        let hash = myground::auth::hash_password(pass).expect("Failed to hash password");
        let auth_cfg = myground::config::AuthConfig {
            username: user.to_string(),
            password_hash: hash,
            cli_token_hash: None,
            api_keys: vec![],
        };
        myground::config::save_auth_config(&state.data_dir, &auth_cfg)
            .expect("Failed to save auth config");
        println!("Auth configured from CLI flags.");
    }

    // Set up Tailscale from CLI flag (key is one-time, not stored)
    if let Some(key) = tailscale_key {
        let ts_cfg = myground::config::TailscaleConfig {
            enabled: true,
            auth_key: None, // Not stored
            tailnet: None,
        };
        myground::config::save_tailscale_config(&state.data_dir, &ts_cfg)
            .expect("Failed to save Tailscale config");
        println!("Tailscale configured from CLI flag.");

        if let Err(e) = myground::tailscale::ensure_exit_node(&state.data_dir, Some(key)).await {
            eprintln!("Warning: failed to start exit node: {e}");
        } else {
            println!("Exit node started.");
        }
    }
}

// ── Tailscale commands ──────────────────────────────────────────────────

async fn cmd_tailscale_status(state: &myground::AppState) {
    let ts_cfg = myground::config::try_load_tailscale(&state.data_dir);

    println!("Tailscale: {}", if ts_cfg.enabled { "enabled" } else { "disabled" });

    if ts_cfg.enabled {
        let exit_running = myground::tailscale::is_exit_node_running().await;
        println!("Exit Node: {}", if exit_running { "running" } else { "stopped" });

        if let Some(ref tailnet) = ts_cfg.tailnet {
            println!("Tailnet: {tailnet}");
        } else if let Some(tn) = myground::tailscale::detect_tailnet().await {
            println!("Tailnet: {tn} (auto-detected)");
        } else {
            println!("Tailnet: not yet detected");
        }

        let installed = myground::config::list_installed_services(&state.data_dir);
        if !installed.is_empty() {
            println!("\nServices on tailnet:");
            for id in &installed {
                let svc_state = myground::config::load_service_state(&state.data_dir, id)
                    .unwrap_or_default();
                let sidecar_running = myground::tailscale::is_sidecar_running(id).await;
                let status = if svc_state.tailscale_disabled {
                    "disabled"
                } else if sidecar_running {
                    "running"
                } else {
                    "stopped"
                };
                if let Some(ref tn) = ts_cfg.tailnet {
                    println!("  {id} [{status}] → https://myground-{id}.{tn}");
                } else {
                    println!("  {id} [{status}] → (tailnet not detected yet)");
                }
            }
        }
    }
}

async fn cmd_tailscale_enable(state: &myground::AppState, auth_key: &str) {
    let ts_cfg = myground::config::TailscaleConfig {
        enabled: true,
        auth_key: None, // Not stored
        tailnet: None,
    };
    if let Err(e) = myground::config::save_tailscale_config(&state.data_dir, &ts_cfg) {
        fatal(format!("Failed to save config: {e}"));
    }
    println!("Tailscale enabled.");

    println!("Starting exit node...");
    match myground::tailscale::ensure_exit_node(&state.data_dir, Some(auth_key)).await {
        Ok(()) => println!("Exit node running."),
        Err(e) => fatal(format!("Failed to start exit node: {e}")),
    }
}

async fn cmd_tailscale_disable(state: &myground::AppState) {
    let mut ts_cfg = myground::config::try_load_tailscale(&state.data_dir);
    ts_cfg.enabled = false;
    if let Err(e) = myground::config::save_tailscale_config(&state.data_dir, &ts_cfg) {
        fatal(format!("Failed to save config: {e}"));
    }

    println!("Stopping exit node...");
    let _ = myground::tailscale::stop_exit_node(&state.data_dir).await;
    println!("Tailscale disabled.");
}

// ── Formatting ──────────────────────────────────────────────────────────────

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    const TIB: u64 = GIB * 1024;

    if bytes >= TIB {
        format!("{:.1}T", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.1}G", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1}M", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1}K", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes}B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0B");
    }

    #[test]
    fn format_bytes_plain_bytes() {
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(1023), "1023B");
    }

    #[test]
    fn format_bytes_kib() {
        assert_eq!(format_bytes(1024), "1.0K");
        assert_eq!(format_bytes(1536), "1.5K");
    }

    #[test]
    fn format_bytes_mib() {
        assert_eq!(format_bytes(1024 * 1024), "1.0M");
        assert_eq!(format_bytes(10 * 1024 * 1024), "10.0M");
    }

    #[test]
    fn format_bytes_gib() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0G");
        assert_eq!(format_bytes(500 * 1024 * 1024 * 1024), "500.0G");
    }

    #[test]
    fn format_bytes_tib() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.0T");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024 * 1024), "2.0T");
    }
}
