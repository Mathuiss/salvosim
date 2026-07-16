use clap::{Arg, Command};

use crate::error::SalvoSimError;

mod config;
mod engine;
mod error;
mod models;
mod web;

fn main() -> Result<(), SalvoSimError> {
    let cmd = Command::new("salvosim").about(r#"

      /$$$$$$   /$$$$$$  /$$    /$$    /$$  /$$$$$$   /$$$$$$  /$$$$$$ /$$      /$$
     /$$__  $$ /$$__  $$| $$   | $$   | $$ /$$__  $$ /$$__  $$|_  $$_/| $$$    /$$$
    | $$  \__/| $$  \ $$| $$   | $$   | $$| $$  \ $$| $$  \__/  | $$  | $$$$  /$$$$
    |  $$$$$$ | $$$$$$$$| $$   |  $$ / $$/| $$  | $$|  $$$$$$   | $$  | $$ $$/$$ $$
     \____  $$| $$__  $$| $$    \  $$ $$/ | $$  | $$ \____  $$  | $$  | $$  $$$| $$
     /$$  \ $$| $$  | $$| $$     \  $$$/  | $$  | $$ /$$  \ $$  | $$  | $$\  $ | $$
    |  $$$$$$/| $$  | $$| $$$$$$$$\  $/   |  $$$$$$/|  $$$$$$/ /$$$$$$| $$ \/  | $$
     \______/ |__/  |__/|________/ \_/     \______/  \______/ |______/|__/     |__/
                                                                                   
    Simulate missile combat with Hughe's salvo model based on stochastic probability or monte-carlo simulation.
    "#).subcommand(
        Command::new("serve")
            .about("Launch the web interface")
            .arg(
                Arg::new("addr")
                    .default_value("0.0.0.0:8080")
                    .help("Address to bind (e.g. 0.0.0.0:8080)"),
            ),
    ).arg(
        Arg::new("file")
            .default_value("scenario.toml")
            .help("Path to scenario file."),
    );

    let matches = cmd.get_matches();

    if let Some(("serve", sub_m)) = matches.subcommand() {
        let addr = sub_m.get_one::<String>("addr").unwrap();
        let rt = rocket::tokio::runtime::Runtime::new()
            .map_err(|e| SalvoSimError::msg(format!("Failed to start async runtime: {e}")))?;
        rt.block_on(web::serve(addr))
            .map_err(|e| SalvoSimError::msg(format!("Server error: {e}")))?;
        Ok(())
    } else {
        let file = matches.get_one::<String>("file").unwrap();
        let config = config::load_scenario(file)?;

        let result = engine::run_monte_carlo(&config);
        engine::print_result(&result);

        Ok(())
    }
}
