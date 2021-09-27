use std::ffi::CString;
use std::io::{Read, Write};
use std::ptr::null_mut;

use matryoshka::Status;
use test_case::test_case;

#[test]
fn test_loading() {
    let database_path = CString::new(":memory:").expect("Valid database path");
    let mut status: *mut Status = null_mut();
    let file_system = unsafe { matryoshka::Load(database_path.as_ptr(), &mut status) };

    assert_eq!(status, null_mut());
    assert!(!file_system.is_null());

    unsafe {
        matryoshka::DestroyFileSystem(file_system);
    }
}

#[test_case("folder/file", &[], -1; "0 bytes, chunk size m1")]
#[test_case("folder/file", &[], 0; "0 bytes, chunk size 0")]
#[test_case("folder/file", &[], 1; "0 bytes, chunk size 1")]
#[test_case("folder/file", &[42u8, 43, 44], -1; "3 bytes, chunk size m1")]
#[test_case("folder/file", &[42u8, 43, 44], 0; "3 bytes, chunk size 0")]
#[test_case("folder/file", &[42u8, 43, 44], 1; "3 bytes, chunk size 1")]
#[test_case("folder/file", &[42u8, 43, 44], 3; "3 bytes, chunk size 3")]
#[test_case("folder/file", &[42u8, 43, 44], 4; "3 bytes, chunk size 4")]
fn test_io(inner_path: &str, data: &[u8], chunk_size: i32) {
    let database_path = CString::new(":memory:").expect("Valid database path");
    let inner_path = CString::new(inner_path).expect("Valid database path");
    let file_system = unsafe { matryoshka::Load(database_path.as_ptr(), null_mut()) };
    assert!(!file_system.is_null());

    let tmp_dir = tempfile::TempDir::new().expect("Unable to create temporary directory");
    {
        // Create and fill input file
        let mut input_path = tmp_dir.path().to_path_buf();
        input_path.push("input.file");
        {
            let mut input_file =
                std::fs::File::create(input_path.as_path()).expect("Creating input file failed");
            assert_eq!(
                input_file.write(data).expect("Writing input file failed"),
                data.len()
            );
        }

        // Push input file to virtual file system
        let file_handle = unsafe {
            let input_file_path =
                CString::new(input_path.to_str().expect("Invalid TMP path")).expect("NULL in path");
            matryoshka::Push(
                file_system,
                inner_path.as_ptr(),
                input_file_path.as_ptr(),
                chunk_size,
                null_mut(),
            )
        };
        assert!(!file_handle.is_null(), "Push failed");

        // Check the size
        assert_eq!(
            unsafe { matryoshka::GetSize(file_system, file_handle) },
            data.len() as i32
        );

        // Pull file from virtual file system
        let mut output_path = tmp_dir.path().to_path_buf();
        output_path.push("output.file");

        let pull_status = unsafe {
            let output_file_path = CString::new(output_path.to_str().expect("Invalid TMP path"))
                .expect("NULL in path");
            matryoshka::Pull(file_system, file_handle, output_file_path.as_ptr())
        };
        assert!(pull_status.is_null(), "Pull failed");

        let mut output_file = std::fs::File::open(output_path).expect("Valid output file missing");
        let mut output_buffer = Vec::new();
        assert_eq!(
            output_file
                .read_to_end(&mut output_buffer)
                .expect("Reading local file failed"),
            data.len()
        );
        assert_eq!(&output_buffer[..], data);

        // Test delete
        assert_eq!(unsafe { matryoshka::Delete(file_system, file_handle) }, 1);
        assert_eq!(unsafe { matryoshka::Delete(file_system, file_handle) }, 0);
    }
}
