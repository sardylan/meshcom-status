mod app;
mod aprs;
mod chat;
mod clock;
mod config;
mod console;
mod errors;
mod frame;
mod graph;
mod import;
mod kiss;
mod logging;
mod node_http;
mod retry;
mod stats;
mod store;
mod view;
mod web;

use std::process::ExitCode;

use config::Action;
use errors::{Error, Result};

fn main() -> ExitCode {
    match program() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn program() -> Result<()> {
    let Some(cli) = config::read_cli()? else {
        return Ok(());
    };
    let (config, notes) = config::resolve(&cli)?;
    logging::start(config.log.level)?;
    notes.iter().for_each(|n| tracing::info!("{n}"));

    let action = config::action(&cli);
    if matches!(action, Action::SaveThenRun | Action::DumpThenExit) {
        config::write_file(&config)?;
        tracing::info!("configuration written to {}", config.config_path.display());
    }
    if action == Action::DumpThenExit {
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Error::Runtime)?;
    match action {
        Action::Import(path) => runtime.block_on(app::import(config, &path)),
        Action::MigrateThenExit => runtime.block_on(app::migrate(config)),
        _ => runtime.block_on(app::run(config)),
    }
}
