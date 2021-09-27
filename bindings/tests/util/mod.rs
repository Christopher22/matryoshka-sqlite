use std::io::Result;
use std::process::{Command, Output};

pub fn execute_with_shared_lib(command: &mut Command) -> Result<Output> {
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

    command
        .env(
            dynamic_lib_env_var,
            dylib_path
                .parent()
                .expect("Shared library has no parent")
                .to_str()
                .expect("Invalid path to shared library"),
        )
        .output()
}
