use cbindgen::DocumentationStyle;
use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let output_file: PathBuf = [
        crate_dir.as_ref(),
        "..",
        "target",
        env::var("PROFILE").expect("PROFILE missing").as_ref(),
        "matryoshka.h",
    ]
    .iter()
    .collect();

    let config = cbindgen::Config {
        language: cbindgen::Language::Cxx,
        include_guard: Some(String::from("MATRYOSHKA")),
        documentation_style: DocumentationStyle::Doxy,
        ..Default::default()
    };

    cbindgen::generate_with_config(&crate_dir, config)
        .unwrap()
        .write_to_file(output_file);

    // On Microsoft Windows: Embed metadata such as the version into the library
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set("ProductName", "matryoshka");
        res.compile().unwrap();
    }
}
