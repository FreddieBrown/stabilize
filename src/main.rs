#![warn(rust_2018_idioms)]

use structopt::{self, StructOpt};

use stabilize::run;
use stabilize::Opt;

fn main() {
    let opt = Opt::from_args();
    let code = {
        if let Err(e) = run(opt) {
            eprintln!("ERROR: {}", e);
            1
        } else {
            0
        }
    };
    println!("Hello World!");
    ::std::process::exit(code);
}