mod app;
mod cli;
mod config;
mod output;
mod scan;

use clap::{CommandFactory, Parser};
use cli::Cli;

fn main() {
    let args = Cli::parse();

    let options = args.scan_options();
    let filter = args
        .scan_filter()
        .unwrap_or_else(|e| exit_error(&e));

    if args.select {
        if args.folder.is_some() || args.output.is_some() {
            exit_error("--select cannot be used with path arguments");
        }
        app::run_with_dialogs(&options, &filter);
        return;
    }

    let Some(folder) = args.folder.clone() else {
        Cli::command().print_help().unwrap();
        return;
    };

    let folder = cli::normalize_path(folder);
    let output_path = cli::resolve_output_path(args.output.clone(), &folder, options.format);

    app::run(&folder, &output_path, &options, &filter);
}

/// Print an error to stderr and exit with a non-zero status.
pub fn exit_error(message: &str) -> ! {
    eprintln!("Error: {message}");
    std::process::exit(1);
}
