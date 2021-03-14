use std::env;
use std::path::PathBuf;
use cbindgen::DocumentationStyle;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let output_file: PathBuf = [
        crate_dir.as_ref(),
        "target",
        env::var("PROFILE").expect("PROFILE missing").as_ref(),
        "matryoshka.h"
    ].iter().collect();

    let mut config: cbindgen::Config = Default::default();
    config.language = cbindgen::Language::Cxx;
    config.include_guard = Some(String::from("MATRYOSHKA"));
    config.documentation_style = DocumentationStyle::Doxy;
    cbindgen::generate_with_config(&crate_dir, config)
        .unwrap()
        .write_to_file(output_file);
}
