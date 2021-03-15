use std::path::PathBuf;
use std::process::Command;

mod util;

#[test]
fn test_dotnet_binding() {
    let dotnet_binding: PathBuf = [env!("CARGO_MANIFEST_DIR"), "bindings", "dotnet"]
        .iter()
        .collect();
    assert!(dotnet_binding.is_dir(), ".NET binding not found");

    let output = util::execute_with_shared_lib(Command::new("dotnet").args(&[
        "test",
        dotnet_binding.to_str().expect("Invalid module path"),
        "-v",
        "detailed",
    ]))
    .expect("Running .NET tests failed.");

    if !output.status.success() {
        panic!(
            ".NET test failed with code {}: {}",
            output.status.code().expect("No error code"),
            String::from_utf8(output.stderr).expect("Invalid STDERR")
        );
    }
}
