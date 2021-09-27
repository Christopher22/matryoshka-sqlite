use std::path::PathBuf;
use std::process::Command;

mod util;

#[test]
fn test_python_binding() {
    let mut python_module: PathBuf = [env!("CARGO_MANIFEST_DIR"), "bindings", "python"]
        .iter()
        .collect();
    assert!(python_module.is_dir(), "Python binding not found");
    python_module.push("__main__.py");

    let output = util::execute_with_shared_lib(
        Command::new("python").arg(python_module.to_str().expect("Invalid module path")),
    )
    .expect("Running python tests failed - is Python correctly installed?");

    if !output.status.success() {
        panic!(
            "{}",
            String::from_utf8(output.stderr).expect("Invalid STDERR")
        );
    }
}
