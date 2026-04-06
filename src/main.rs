mod app;
mod backend;
mod error;
mod installed_state;
mod rpm_info;
mod state_logic;
mod ui;

use tracing_subscriber::{EnvFilter, fmt};

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).try_init();
}

fn main() {
    init_tracing();
    app::run();
}
