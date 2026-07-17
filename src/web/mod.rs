pub mod handlers;
pub mod scenario;

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::RwLock;

use rand::RngExt;
use rocket::config::Config;
use rocket::fs::FileServer;

use crate::engine::StepwiseSimulation;
use crate::models::{RunDefender, SimulationResult, StepRecord};

/// An interactive step-through session with full step history.
pub struct InteractiveSession {
    pub sim: StepwiseSimulation,
    pub history: Vec<StepRecord>,
    pub initial_snapshot: Vec<RunDefender>,
    pub scenario_file: String,
}

/// Shared application state held by Rocket.
pub struct AppState {
    pub results: RwLock<HashMap<String, SimulationResult>>,
    pub sessions: RwLock<HashMap<String, InteractiveSession>>,
    pub session_scenario: RwLock<HashMap<String, String>>,
    pub scenarios_dir: PathBuf,
}

/// Generate a short random session id.
pub fn generate_session_id() -> String {
    // Use random_range for a u64 value (available via RngExt from the root).
    let mut rng = rand::rng();
    let suffix: u64 = rng.random_range(0..u64::MAX);
    format!("sess_{suffix:016x}")
}

/// Launch the Rocket web server and block until shutdown.
pub async fn serve(addr: &str) -> Result<(), rocket::Error> {
    let (ip, port) = parse_addr(addr);
    let config = Config {
        address: ip,
        port,
        ..Config::default()
    };

    // Ensure the scenarios directory exists.
    let scenarios_dir = PathBuf::from("scenarios");
    if !scenarios_dir.exists() {
        std::fs::create_dir_all(&scenarios_dir).expect("failed to create scenarios/ directory");
        // Copy example scenarios from the project root if available.
        for example in &["scenario-mc.toml", "scenario-st.toml"] {
            let src = PathBuf::from(example);
            if src.exists() {
                let dst = scenarios_dir.join(example);
                if !dst.exists() {
                    std::fs::copy(&src, &dst).ok();
                }
            }
        }
    }

    // Diagnose what the app will serve from the scenarios directory.
    let abs = std::fs::canonicalize(&scenarios_dir).unwrap_or_else(|_| scenarios_dir.clone());
    eprintln!("[salvosim] Scenarios directory: {}", abs.display());
    match std::fs::read_dir(&scenarios_dir) {
        Ok(entries) => {
            let count = entries.flatten().count();
            eprintln!("[salvosim] Found {count} file(s) in scenarios/");
        }
        Err(e) => eprintln!("[salvosim] Cannot read scenarios/: {e}"),
    }

    // Ensure the static directory exists and has the logo.
    let static_dir = PathBuf::from("static");
    if !static_dir.exists() {
        std::fs::create_dir_all(&static_dir).expect("failed to create static/ directory");
    }
    let logo_src = PathBuf::from("templates/logo.png");
    let logo_dst = static_dir.join("logo.png");
    if logo_src.exists() && !logo_dst.exists() {
        match std::fs::copy(&logo_src, &logo_dst) {
            Ok(_) => eprintln!(
                "[salvosim] Copied logo to {}/logo.png",
                static_dir.display()
            ),
            Err(e) => eprintln!("[salvosim] Could not copy logo: {e}"),
        }
    }

    let rocket = rocket::custom(config)
        .manage(AppState {
            results: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            session_scenario: RwLock::new(HashMap::new()),
            scenarios_dir,
        })
        .mount("/static", FileServer::from("static"))
        .mount("/", handlers::routes())
        .attach(rocket_dyn_templates::Template::fairing());

    rocket.launch().await.map(|_| ())
}

/// Parse `host:port` into an `IpAddr` and `u16`.
fn parse_addr(addr: &str) -> (IpAddr, u16) {
    let parts: Vec<&str> = addr.split(':').collect();
    let ip: IpAddr = parts[0]
        .parse()
        .unwrap_or_else(|_| IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)));
    let port: u16 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(8080);
    (ip, port)
}
