mod app;
mod cli;
mod config;
mod error;
mod output;
mod scan;

use std::process::ExitCode;

use clap::Parser;
use cli::Cli;
use error::Error;

fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> Result<(), Error> {
    let cli = Cli::parse();
    let options = cli.scan_options();
    let filter = cli.scan_filter()?;

    if cli.select {
        if cli.folder.is_some() || cli.output.is_some() {
            return Err(Error::SelectWithPaths);
        }
        return app::run_with_dialogs(&options, &filter);
    }

    let Some(folder) = cli.folder else {
        return Err(Error::MissingFolder);
    };

    let folder = cli::normalize_path(folder);
    let output_path = cli::resolve_output_path(cli.output, &folder, options.format);
    app::run(&folder, &output_path, &options, &filter)
}
