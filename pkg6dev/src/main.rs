#[macro_use]
extern crate failure_derive;

use clap::app_from_crate;
use clap::{Arg, App};
use libips::actions::{File, Manifest};

mod errors {
    use failure::Error;
    use std::result::Result as StdResult;

    pub type Result<T> = StdResult<T, Error>;
}

use errors::Result;

fn main() {
    let opts = app_from_crate!().arg(Arg::new("proto_dir")
        .short('p')
        .long("proto-dir")
        .value_name("PROTO_DIR")
        .about("The Prototype directory where files are located after build")
        .takes_value(true)//.required(true)
        .default_value("../sample_data/pkgs/cups/build/prototype/i386")
    ).subcommand(App::new("diff-manifests")
        .about("shows differences between two manifests")
        .arg(Arg::new("manifests")
            .value_name("MANIFESTS")
            .multiple(true)
            .number_of_values(2)
        )
    ).get_matches();

    let proto_dir = opts.value_of("proto_dir").expect("proto_dir is a mandatory variable. clap::Arg::required must be true");

    //let manifests: Vec<_> = opts.values_of("manifests").unwrap().collect();

    //let files = find_removed_files(String::from(&manifests[0]), String::from(&manifests[1])).unwrap();
    let _ = find_removed_files(String::from("../sample_data/pkgs/cups/cups.p5m"), String::from("../sample_data/pkgs/cups/manifests/sample-manifest.p5m")).unwrap();

}

fn find_removed_files(manifest_file: String, other_manifest_file: String) -> Result<Vec<File>> {
    let manifest = Manifest::parse_file(manifest_file)?;
    let other_manifest = Manifest::parse_file(other_manifest_file)?;

    println!("{:#?}", manifest);
    println!("{:#?}", other_manifest);


    Ok(vec![File::default()])
}
