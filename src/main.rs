use clap::{App, load_yaml};

use builder::*;

pub(crate) mod builder;
pub(crate) mod entry;

fn main() {
    let yaml = load_yaml!("../cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    DocsetBuilder::build(matches.value_of("dir")
                             .map(|s| s.to_string()),
                         matches.value_of("out")
                             .map(|s| s.to_string()));
}