#![warn(rust_2018_idioms)]

use structopt::{self, StructOpt};
use stabilize::run;
use stabilize::Opt;


fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();
    let code = {
        if let Err(e) = run(opt) {
            eprintln!("ERROR: {}", e);
            1
        } else {
            0
        }
    };
    ::std::process::exit(code);
}