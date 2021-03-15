use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_python_binding() {
    let dynamic_lib_env_var = if cfg!(target_os = "windows") {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else if cfg!(target_os = "linux") {
        "LD_LIBRARY_PATH"
    } else {
        panic!("Unsupported OS!")
    };

    let dylib_path = test_cdylib::build_current_project();

    let mut python_module: PathBuf = [env!("CARGO_MANIFEST_DIR"), "bindings", "python"]
        .iter()
        .collect();
    assert!(python_module.is_dir(), "Python binding not found");
    python_module.push("__main__.py");

    let output = Command::new("python")
        .env(
            dynamic_lib_env_var,
            dylib_path
                .parent()
                .expect("cdylib has not parent")
                .to_str()
                .expect("Invalid dylib path"),
        )
        .arg(python_module.to_str().expect("Invalid module path"))
        .output()
        .expect("Running python tests failed - is Python correctly installed?");

    if !output.status.success() {
        panic!(String::from_utf8(output.stderr).expect("Invalid STDERR"));
    }
}
