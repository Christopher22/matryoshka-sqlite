#![allow(clippy::missing_safety_doc)] // Well, using C-pointers *is* unsafe...

extern crate static_vcruntime;

use std::convert::TryInto;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr::{null, null_mut};

use matryoshka_sqlite::{
    errors::{DatabaseError, Error},
    Database, File, FileSystem as RawFileSystem, Handle as RawHandle,
};

struct Environment(*mut *mut Status);

impl From<*mut *mut Status> for Environment {
    fn from(value: *mut *mut Status) -> Self {
        Self(value)
    }
}

impl Environment {
    pub fn execute<T, C: FnOnce() -> Result<T, String>>(self, body: C) -> *mut T {
        match body() {
            Ok(value) => Box::into_raw(Box::new(value)),
            Err(error) => {
                if !self.0.is_null() {
                    let status = Environment::create_status(&error);
                    unsafe {
                        std::ptr::write(self.0, status);
                    }
                }
                null_mut()
            }
        }
    }

    pub fn create_status<T: AsRef<str>>(description: T) -> *mut Status {
        let message = CString::new(description.as_ref()).expect("Found NULL");
        Box::into_raw(Box::new(Status(message)))
    }

    pub fn parse_str<'a>(c_string: *const c_char) -> Result<&'a str, String> {
        (!c_string.is_null())
            .then(|| unsafe { CStr::from_ptr(c_string) })
            .ok_or_else(|| String::from("Path not specified"))
            .and_then(|raw_str| {
                raw_str
                    .to_str()
                    .map_err(|_| String::from("Path contains invalid UTF8"))
            })
    }

    pub fn destroy<T>(pointer: *mut T) {
        if pointer.is_null() {
            return;
        }
        unsafe {
            Box::from_raw(pointer);
        }
    }
}

/// Then virtual file system.
pub struct FileSystem(RawFileSystem<Database>);

/// The status of the operation.
pub struct Status(CString);

/// The handle to a file.
pub struct FileHandle(RawHandle);

/// Open a SQLite database containing the Matryoshka virtual file system.
///
/// @param path The path to the Matryoshka SQlite database.
///
/// @param status Contains the error code of the failure if and only if the return value is nullptr. Setting this value to nullptr is safe and will not save the error code.
///
/// @return A pointer to the virtual file system or nullptr on failure.
#[no_mangle]
pub unsafe extern "C" fn Load(path: *const c_char, status: *mut *mut Status) -> *mut FileSystem {
    Environment::from(status).execute(|| {
        let path = Environment::parse_str(path)?;

        let database = Database::open(path).map_err(|error| {
            let sqlite_error: Result<DatabaseError, ()> = error.try_into();
            match sqlite_error {
                Ok(error) => format!("{}", error),
                Err(_) => String::from("Unable to open database"),
            }
        })?;

        Ok(FileSystem(
            RawFileSystem::load(database, true).map_err(|error| error.error_message())?,
        ))
    })
}

/// Destroy a file system.
///
/// @param file_system The virtual file system. Passing nullptr is a safe no-op.
#[no_mangle]
pub unsafe extern "C" fn DestroyFileSystem(file_system: *mut FileSystem) {
    Environment::destroy(file_system)
}

/// Destroy a status.
///
/// @param status The status. Passing nullptr is a safe no-op.
#[no_mangle]
pub unsafe extern "C" fn DestroyStatus(status: *mut Status) {
    Environment::destroy(status)
}

/// Destroy a file handle.
///
/// @param file_handle The file handle. Passing nullptr is a safe no-op.
#[no_mangle]
pub unsafe extern "C" fn DestroyFileHandle(file_handle: *mut FileHandle) {
    Environment::destroy(file_handle)
}

/// Return the error message associated with a status.
///
/// @param status The status of interest.
///
/// @return A human-readable description of the failure.
#[no_mangle]
pub unsafe extern "C" fn GetMessage(status: *const Status) -> *const c_char {
    match status.as_ref() {
        Some(value) => value.0.as_ptr(),
        None => null(),
    }
}

/// Open a existing file on the virtual file system.
///
/// @param file_system A pointer to the virtual file system.
///
/// @param path The (inner) path on the virtual file system (mind the forward slashes as separators!)
///
/// @param status Contains the error code of the failure if and only if the return value is nullptr. Setting this value to nullptr is safe and will not save the error code.
///
/// @return A handle to the file or nullptr at failure.
#[no_mangle]
pub unsafe extern "C" fn Open(
    file_system: *mut FileSystem,
    path: *const c_char,
    status: *mut *mut Status,
) -> *mut FileHandle {
    Environment::from(status).execute(|| {
        let file_system = file_system
            .as_ref()
            .ok_or_else(|| String::from("File system not specified"))?;
        let inner_path = Environment::parse_str(path)?;
        let file = File::load(&file_system.0, inner_path).map_err(|error| error.error_message())?;
        Ok(FileHandle(file.handle()))
    })
}

/// Push a file to the virtual file system.
///
/// @param file_system A pointer to the virtual file system.
///
/// @param inner_path The inner path on the virtual file system (mind the forward slashes as separators!)
///
/// @param file_path The path on the real file system.
///
/// @param chunk_size The proposed chunk size. Negative values will let the virtual file system choose.
///
/// @param status Contains the error code of the failure if and only if the return value is nullptr. Setting this value to nullptr is safe and will not save the error code.
///
/// @return A handle to the newly created file or nullptr on failure.
#[no_mangle]
pub unsafe extern "C" fn Push(
    file_system: *mut FileSystem,
    inner_path: *const c_char,
    file_path: *const c_char,
    chunk_size: c_int,
    status: *mut *mut Status,
) -> *mut FileHandle {
    Environment::from(status).execute(|| {
        let file_system = file_system
            .as_mut()
            .ok_or_else(|| String::from("File system not specified"))?;
        let inner_path = Environment::parse_str(inner_path)?;

        let file_path = Environment::parse_str(file_path)?;
        let local_file = match std::fs::File::open(file_path) {
            Ok(file) => file,
            Err(error) => {
                return Err(format!("Open file failed: {:?}", error));
            }
        };

        let chunk_size = std::cmp::max(0, chunk_size) as usize;
        let file = File::create(&mut file_system.0, inner_path, local_file, chunk_size)
            .map_err(|error| error.error_message())?;
        Ok(FileHandle(file.handle()))
    })
}

/// Pull a file from the database into the virtual file system.
///
/// @param file_system A pointer to the virtual file system.
///
/// @param inner_path The inner path on the virtual file system (mind the forward slashes as separators!)
///
/// @param file_path The path on the real file system.
///
/// @return A error ocurring during operation or nullptr on success.
#[no_mangle]
pub unsafe extern "C" fn Pull(
    file_system: *mut FileSystem,
    handle: *const FileHandle,
    file_path: *const c_char,
) -> *mut Status {
    let file_system = match file_system.as_mut() {
        Some(file_system) => file_system,
        None => {
            return Environment::create_status("File system not specified");
        }
    };

    let handle = match handle.as_ref() {
        Some(handle) => handle,
        None => {
            return Environment::create_status("File handle not specified");
        }
    };

    let local_path = match Environment::parse_str(file_path) {
        Ok(local_path) => local_path,
        Err(error) => {
            return Environment::create_status(error);
        }
    };

    let virtual_file: File<_> = match (&file_system.0, handle.0).try_into() {
        Ok(file) => file,
        Err(error) => {
            return Environment::create_status(error.error_message());
        }
    };

    let local_file = match std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(local_path)
    {
        Ok(file) if file.set_len(virtual_file.len() as u64).is_ok() => file,
        _ => {
            return Environment::create_status("Unable to create the local file");
        }
    };

    match virtual_file.random_read(local_file, 0, virtual_file.len()) {
        Ok(num_bytes) if num_bytes == virtual_file.len() => null_mut(),
        Err(error) => Environment::create_status(error.error_message()),
        _ => Environment::create_status("Less than expected bytes were written."),
    }
}

/// Returns the size of a file.
///
/// @param file_system A pointer to the virtual file system.
///
/// @param file A handle to the file.
///
/// @return File size in bytes.
#[no_mangle]
pub unsafe extern "C" fn GetSize(
    file_system: *const FileSystem,
    file_handle: *const FileHandle,
) -> c_int {
    let file_system = match file_system.as_ref() {
        Some(file_system) => file_system,
        None => {
            return -1;
        }
    };

    let file_handle = match file_handle.as_ref() {
        Some(file_system) => file_system,
        None => {
            return -1;
        }
    };

    let file: File<_> = match (&file_system.0, file_handle.0).try_into() {
        Ok(file) => file,
        Err(_) => {
            return -1;
        }
    };

    file.len() as c_int
}

/// Delete a file. The file handle must not be used after the call but still needs to be freed.
///
/// @param file_system A pointer to the virtual file system.
///
/// @param file A handle to the file.
///
/// @return 1 if operation was successful, 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn Delete(file_system: *mut FileSystem, file: *mut FileHandle) -> c_int {
    let file_system = match file_system.as_ref() {
        Some(file_system) => file_system,
        None => {
            return 0;
        }
    };

    let file_handle = match file.as_ref() {
        Some(file_system) => file_system.0,
        None => {
            return 0;
        }
    };

    let file: File<_> = match (&file_system.0, file_handle).try_into() {
        Ok(file) => file,
        Err(_) => {
            return 0;
        }
    };

    match file.delete() {
        true => 1,
        false => 0,
    }
}

/// Search for a specific file(s).
///
/// @param file_system A pointer to the virtual file system.
///
/// @param path The path supporting glob-like placeholders.
///
/// @param callback A callback for each path found.
///
/// @return The number of paths found.
#[no_mangle]
pub unsafe extern "C" fn Find(
    file_system: *mut FileSystem,
    path: *const c_char,
    callback: unsafe extern "C" fn(*const c_char),
) -> c_int {
    let file_system = match file_system.as_ref() {
        Some(file_system) => file_system,
        None => {
            return 0;
        }
    };
    let path = match Environment::parse_str(path) {
        Ok(path) => path,
        _ => {
            return 0;
        }
    };

    let paths: Vec<CString> = match file_system.0.find(path).map(|paths| {
        paths
            .into_iter()
            .map(|path| CString::new(path).expect("NULL found"))
            .collect()
    }) {
        Ok(paths) => paths,
        _ => {
            return 0;
        }
    };

    for path in paths.iter() {
        callback(path.as_ptr());
    }

    paths.len() as c_int
}
