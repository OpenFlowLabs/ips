mod workspace;
mod sources;

use anyhow::anyhow;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use specfile::parse;
use specfile::macros;
use std::collections::HashMap;
use crate::workspace::Workspace;
use anyhow::Result;

enum Verbose{
    Off,
    Some,
    On,
    Debug
}

#[derive(Debug, Parser)]
#[clap(version)]
struct Cli {
    #[clap(subcommand)]
    pub command: Commands,

    #[clap(short, long, env)]
    pub config: Option<PathBuf>,

    #[clap(short, parse(from_occurrences))]
    pub verbose: i8,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Package {
        #[clap(short, long)]
        target: PathBuf,

        #[clap(value_parser)]
        specfile: PathBuf,
    } 
}

fn main() -> Result<()> {
    let cli: Cli = Cli::parse();

    if let Some(c) = cli.config {
        println!("Value for config: {}", c.display());
    }

    let _verbose = match cli.verbose {
        0 => Verbose::Off,
        1 => Verbose::Some,
        2 => Verbose::On,
        3 | _ => Verbose::Debug,
    };

    match cli.command {
        Commands::Package { target, specfile } => {
            run_package_command(specfile, target)?;
        }
    }
    
    Ok(())
}

fn run_package_command<P: AsRef<Path>>(spec_file: P, _target: P) -> Result<()> {
    let content_string = fs::read_to_string(spec_file)?;
    let spec = parse(content_string)?;
    let mut ws = Workspace::new("")?;
    let downloaded = ws.get_sources(spec.sources)?;
    ws.unpack_all_sources(downloaded)?;

    let mut macro_map= HashMap::<String, String>::new();
    for ws_macro in ws.get_macros() {
        macro_map.insert(
            ws_macro.0, 
            ws_macro.1.to_str().ok_or(anyhow!("not string path {}", ws_macro.1.display()))?.to_owned()
        );
    }

    let mp = macros::MacroParser {
        macros: macro_map
    };

    let build_script = mp.parse(spec.build_script)?;
    ws.build(build_script)?;
    ws.package(spec.files)?;

    Ok(())
}
