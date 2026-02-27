use std::collections::HashMap;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "myground", version, about = "Self-hosting platform — hold your ground")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
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
    },
    /// Show status of MyGround and managed services
    Status,
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

fn create_state() -> myground::AppState {
    let data_dir = myground::config::data_dir();
    myground::config::ensure_data_dir(&data_dir).expect("Failed to create data directory");
    myground::AppState::new(data_dir)
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

    match cli.command {
        Some(Commands::Start { port, address }) => {
            let state = create_state();
            myground::serve(state, &address, port).await;
        }
        Some(Commands::Status) => {
            let state = create_state();
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

            // Disk summary
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
        Some(Commands::Service { action }) => {
            let state = create_state();
            match action {
                ServiceAction::List => {
                    cmd_service_list(&state).await;
                }
                ServiceAction::Install { id } => {
                    cmd_service_install(&state, &id).await;
                }
                ServiceAction::Start { id } => {
                    run_service_action(&id, "start", myground::services::start_service(&state.data_dir, &id)).await;
                }
                ServiceAction::Stop { id } => {
                    run_service_action(&id, "stop", myground::services::stop_service(&state.data_dir, &id)).await;
                }
                ServiceAction::Remove { id } => {
                    run_service_action(&id, "remove", myground::services::remove_service(&state.data_dir, &id)).await;
                }
            }
        }
        Some(Commands::Disk { action }) => match action {
            DiskAction::List => {
                cmd_disk_list();
            }
            DiskAction::Health => {
                cmd_disk_health();
            }
        },
        None => {
            let state = create_state();
            myground::serve(state, "0.0.0.0", 8080).await;
        }
    }
}

async fn cmd_service_list(state: &myground::AppState) {
    let installed = myground::config::list_installed_services(&state.data_dir);
    let container_map = myground::docker::get_container_statuses(&state.docker).await;

    println!(
        "{:<15} {:<20} {:<12} {:<10}",
        "ID", "NAME", "INSTALLED", "STATUS"
    );
    println!("{}", "-".repeat(57));

    let mut services: Vec<_> = state.registry.iter().collect();
    services.sort_by_key(|(id, _)| (*id).clone());

    for (id, def) in services {
        let is_installed = installed.contains(id);
        let status = if let Some(containers) = container_map.get(id.as_str()) {
            containers
                .first()
                .map(|c| c.state.clone())
                .unwrap_or_else(|| "unknown".to_string())
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
    let Some(def) = state.registry.get(id) else {
        eprintln!("Unknown service: {id}");
        eprintln!(
            "Available: {}",
            state
                .registry
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        std::process::exit(1);
    };

    println!("Installing {id}...");
    let global_config = myground::config::load_global_config(&state.data_dir)
        .unwrap_or_default();
    match myground::services::install_service(&state.data_dir, def, &HashMap::new(), &global_config).await {
        Ok(()) => println!("Service {id} installed successfully."),
        Err(e) => {
            eprintln!("Failed to install {id}: {e}");
            std::process::exit(1);
        }
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
        Err(e) => {
            eprintln!("Failed to {verb} {id}: {e}");
            std::process::exit(1);
        }
    }
}

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
