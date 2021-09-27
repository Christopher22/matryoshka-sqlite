//! The "safe and rusty" implementation of the virtual file system.

use std::borrow::BorrowMut;
use std::convert::{TryFrom, TryInto};
use std::io::{Error as IoError, ErrorKind, Read, Result as IoResult, Write};

use rusqlite::limits::Limit;
use rusqlite::{
    params, Connection as Database, DatabaseName, Error as RusqliteError, ErrorCode,
    OptionalExtension,
};

use super::errors::{CreationError, DatabaseError, FileSystemError, LoadingError, ReadError};
pub use super::util::Handle;
use super::util::{Availability, MetaData, VirtualPath};
use crate::errors::Error;

mod constants {
    use const_format::formatcp;

    pub const CURRENT_MATRYOSHKA_VERSION: u32 = 0;
    pub const MATRYOSHKA_TABLE: &str = "Matryoshka_Meta_0";
    // One day, that might be derived directly from a const function.
    pub const DATA_TABLE: &str = "Matryoshka_Data";

    pub const FILE_ID: u32 = 1;

    pub const DEFAULT_BYTE_BLOB_SIZE: usize = 33554432; // 32MB

    pub const SQL_CREATE_META: &str = formatcp!(
        "CREATE TABLE {} (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, type INTEGER, flags INTEGER, chunk_size INTEGER NOT NULL)",
        MATRYOSHKA_TABLE
    );
    pub const SQL_CREATE_DATA: &str = formatcp!(
        "CREATE TABLE IF NOT EXISTS {} (chunk_id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL, chunk_num INTEGER NOT NULL, data BLOB NOT NULL, CONSTRAINT unq UNIQUE (file_id, chunk_num), FOREIGN KEY(file_id) REFERENCES {} (id) ON DELETE CASCADE ON UPDATE CASCADE)",
        DATA_TABLE,
        MATRYOSHKA_TABLE
    );
    pub const SQL_CREATE_HANDLE: &str = formatcp!(
        "INSERT INTO {} (path, type, chunk_size) VALUES (?, ?, ?)",
        MATRYOSHKA_TABLE
    );
    pub const SQL_CREATE_BLOB: &str = formatcp!(
        "INSERT INTO {} (file_id, chunk_num, data) VALUES (?, ?, ?)",
        DATA_TABLE
    );
    pub const SQL_GET_HANDLE: &str = formatcp!(
        "SELECT id FROM {} WHERE path = ? AND type = ?",
        MATRYOSHKA_TABLE
    );
    pub const SQL_GLOB: &str = formatcp!(
        "SELECT path FROM {} WHERE path GLOB ? AND type = ?",
        MATRYOSHKA_TABLE
    );
    pub const SQL_SIZE: &str = formatcp!(
        "SELECT COALESCE(SUM(LENGTH(data)), -1) FROM {} WHERE file_id = ?",
        DATA_TABLE
    );
    pub const SQL_DELETE: &str = formatcp!("DELETE FROM {} WHERE id = ?", MATRYOSHKA_TABLE);
    pub const SQL_GET_BLOBS: &str = formatcp!("SELECT chunk_id, chunk_num, {meta}.chunk_size FROM {data}
        INNER JOIN {meta} ON {meta}.id={data}.file_id
        WHERE file_id = :handle AND chunk_num BETWEEN cast((:index / {meta}.chunk_size) as int) AND cast(((:index + :size - 1) / {meta}.chunk_size) as int)
        ORDER BY chunk_num ASC",
        data=DATA_TABLE,
        meta=MATRYOSHKA_TABLE
    );
}

/// A virtual file system in a SQLite database.
#[derive(Debug)]
pub struct FileSystem<D> {
    database: D,
    meta_data: MetaData,
}

impl<D> FileSystem<D>
where
    D: BorrowMut<Database>,
{
    /// Load the virtual file system from an SQLite database.
    pub fn load(
        mut database: D,
        create_file_system: bool,
    ) -> Result<FileSystem<D>, FileSystemError> {
        let meta_data = match MetaData::from_database(database.borrow()) {
            Availability::Available(meta_data)
                if meta_data.version() == constants::CURRENT_MATRYOSHKA_VERSION =>
            {
                Ok(meta_data)
            }
            Availability::Available(meta_data) => {
                Err(FileSystemError::UnsupportedVersion(meta_data.version()))
            }
            Availability::Missing if create_file_system => {
                let transaction = database.borrow_mut().transaction()?;
                transaction.execute(constants::SQL_CREATE_META, [])?;
                transaction.execute(constants::SQL_CREATE_DATA, [])?;
                transaction.commit()?;
                Ok(MetaData::from_version(
                    constants::CURRENT_MATRYOSHKA_VERSION,
                ))
            }
            Availability::Missing => Err(FileSystemError::NoFileSystem),
            Availability::Error(error) => Err(error.into()),
        }?;

        // Pre-compile the primary SQL commands
        const PRECOMPILED_COMMANDS: [&str; 6] = [
            constants::SQL_GET_HANDLE,
            constants::SQL_CREATE_HANDLE,
            constants::SQL_GLOB,
            constants::SQL_SIZE,
            constants::SQL_DELETE,
            constants::SQL_GET_BLOBS,
        ];

        database
            .borrow()
            .set_prepared_statement_cache_capacity(PRECOMPILED_COMMANDS.len());
        for statement in &PRECOMPILED_COMMANDS {
            database
                .borrow()
                .prepare_cached(statement)
                .map_err(|error| FileSystemError::InvalidBaseCommand(statement, error))?;
        }

        Ok(FileSystem {
            database,
            meta_data,
        })
    }

    /// Query the file system for those files with a specific GLOB pattern. Both the '?' and the '*' placeholder are supported
    pub fn find<T: AsRef<str>>(&self, path: T) -> Result<Vec<String>, DatabaseError> {
        let path: VirtualPath = path.as_ref().into();
        let mut handle_query = self
            .database
            .borrow()
            .prepare_cached(constants::SQL_GLOB)
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))?;

        // We must cache the result to avoid lifetime issues.
        let result = handle_query
            .query_map(params![path.as_ref(), constants::FILE_ID], |row| {
                Ok(row.get_unwrap(0))
            })
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))?
            .map(|handle| handle.unwrap()) // The price we have to pay to get a iterator ...
            .collect();

        Ok(result)
    }

    fn create<T: Into<VirtualPath>, R: Read>(
        &mut self,
        path: T,
        mut data: R,
        chunk_size: usize,
    ) -> Result<Handle, CreationError> {
        let max_blob_size = self.database.borrow().limit(Limit::SQLITE_LIMIT_LENGTH);
        let chunk_size = match chunk_size {
            value if value > 0 && value <= max_blob_size as usize => value,
            _ => constants::DEFAULT_BYTE_BLOB_SIZE,
        };

        // Create the transaction to return safely on errors and prepare the statement.
        let transaction = self.database.borrow_mut().transaction()?;

        let handle = {
            let mut create_handle_statement =
                transaction.prepare_cached(constants::SQL_CREATE_HANDLE)?;
            let mut create_blob_statement =
                transaction.prepare_cached(constants::SQL_CREATE_BLOB)?;

            let handle = match create_handle_statement.insert(params![
                path.into().as_ref(),
                constants::FILE_ID,
                chunk_size as i32
            ]) {
                Ok(handle) => handle,
                Err(RusqliteError::SqliteFailure(error, _))
                    if error.code == ErrorCode::ConstraintViolation =>
                {
                    return Err(CreationError::FileExists);
                }
                Err(error) => {
                    return Err(error.into());
                }
            };

            let mut buffer = vec![0u8; chunk_size as usize];
            let mut chunk_index = 0u32;
            loop {
                match data.read(buffer.as_mut()) {
                    Ok(size) => {
                        create_blob_statement.execute(params![
                            handle,
                            chunk_index,
                            &buffer[0..size]
                        ])?;
                        if size != chunk_size {
                            break;
                        }
                        chunk_index += 1;
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => {
                        // Just try again...
                    }
                    Err(error) => {
                        return Err(error.into());
                    }
                }
            }

            handle
        };

        transaction.commit()?;
        Ok(Handle(handle))
    }

    fn open<T: Into<VirtualPath>>(&self, path: T) -> Result<Option<Handle>, DatabaseError> {
        let mut handle_query = self
            .database
            .borrow()
            .prepare_cached(constants::SQL_GET_HANDLE)
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))?;
        handle_query
            .query_row(params![path.into().as_ref(), constants::FILE_ID], |row| {
                Ok(Handle(row.get_unwrap(0)))
            })
            .optional()
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }

    fn read<W: Write>(
        &self,
        handle: Handle,
        mut sink: W,
        index: usize,
        length: usize,
    ) -> Result<usize, ReadError> {
        let index = i64::try_from(index).map_err(|_| ReadError::FileSystemLimits)?;

        // Check length and exit early if not data is of interest
        let length = i64::try_from(length).map_err(|_| ReadError::FileSystemLimits)?;
        if length == 0 {
            return Ok(0);
        }

        // Prepare the statements regarding the blobs
        let mut blobs_statement = self
            .database
            .borrow()
            .prepare_cached(constants::SQL_GET_BLOBS)?;

        // Let SQLite calculate all the key characteristics
        let mut chuck_size: Option<i64> = None;
        let mut blob_iter = blobs_statement
            .query_map(
                &[
                    (":handle", &handle.0),
                    (":index", &index),
                    (":size", &length),
                ],
                |row| {
                    Ok(match chuck_size {
                        Some(chunk_size) => (0usize, row.get_unwrap(0), chunk_size),
                        None => {
                            let raw_chunk_size: i64 = row.get_unwrap(2);
                            let chunk_num: i64 = row.get_unwrap(1);
                            chuck_size = Some(raw_chunk_size);
                            let offset: i64 = index - (chunk_num * raw_chunk_size);
                            (offset as usize, row.get_unwrap(0), raw_chunk_size)
                        }
                    })
                },
            )?
            .map(|blob_index| blob_index.unwrap());

        // Initialize the chunk: Chunk size must always be equal or larger to the biggest blob.
        let first_blob = blob_iter.next().ok_or(ReadError::OutOfBounds)?;
        let mut buffer = vec![0u8; first_blob.2 as usize];

        let mut bytes_read = 0i64;
        let mut blob_cache: Option<rusqlite::blob::Blob> = None;
        for (index, (first_index, blob_id, _)) in
            std::iter::once(first_blob).chain(blob_iter).enumerate()
        {
            let blob = match blob_cache {
                None => self.database.borrow().blob_open(
                    DatabaseName::Main,
                    constants::DATA_TABLE,
                    "data",
                    blob_id,
                    true,
                ),
                Some(mut blob) => blob.reopen(blob_id).map(|_| blob),
            }?;

            let blob_size = blob.size() as i64;
            let mut num_bytes = std::cmp::min(blob_size, length - bytes_read);
            if index == 0 {
                num_bytes = std::cmp::min(blob_size - first_index as i64, num_bytes);
                if num_bytes <= 0 {
                    return Err(ReadError::OutOfBounds);
                }
            }

            // Read data into the buffer
            blob.read_at_exact(&mut buffer[..num_bytes as usize], first_index)?;

            // Copy data to writer
            sink.write_all(&buffer[..num_bytes as usize])?;

            bytes_read += num_bytes;
            blob_cache = Some(blob);
        }

        // Raise an out-of-bound error if the length it too large.
        match bytes_read == length {
            true => Ok(bytes_read as usize),
            false => Err(ReadError::OutOfBounds),
        }
    }

    fn delete(&self, handle: Handle) -> Result<usize, DatabaseError> {
        let mut delete_query = self
            .database
            .borrow()
            .prepare_cached(constants::SQL_DELETE)
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))?;
        delete_query
            .execute(params![handle.0])
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }

    fn size(&self, handle: Handle) -> Result<Option<usize>, DatabaseError> {
        let mut handle_query = self
            .database
            .borrow()
            .prepare_cached(constants::SQL_SIZE)
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))?;
        handle_query
            .query_row(params![handle.0], |row| {
                let raw_size: i64 = row.get_unwrap(0);
                match raw_size >= 0 {
                    true => Ok(Some(raw_size as usize)),
                    false => Ok(None),
                }
            })
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }
}

/// A file stored in the virtual file system.
#[derive(Debug)]
pub struct File<'a, D> {
    file_system: &'a FileSystem<D>,
    handle: Handle,
    size: usize,
    current_index: usize,
}

impl<'a, D> File<'a, D>
where
    D: BorrowMut<Database>,
{
    /// Create a file in the virtual file system.
    pub fn create<T: AsRef<str>, R: Read>(
        file_system: &'a mut FileSystem<D>,
        path: T,
        data: R,
        chunk_size: usize,
    ) -> Result<File<'a, D>, CreationError> {
        let handle = file_system.create(path.as_ref(), data, chunk_size)?;
        let size = file_system
            .size(handle)
            .map_err(CreationError::DatabaseError)?
            .expect("Missing file size for existing file");
        Ok(File {
            file_system,
            handle,
            size,
            current_index: 0,
        })
    }

    /// Load a file from the virtual file system.
    pub fn load<T: AsRef<str>>(
        file_system: &'a FileSystem<D>,
        path: T,
    ) -> Result<File<'a, D>, LoadingError> {
        match file_system.open(path.as_ref()) {
            Ok(Some(handle)) => Ok(File {
                file_system,
                handle,
                size: file_system
                    .size(handle)
                    .map_err(LoadingError::DatabaseError)?
                    .expect("Missing file size for existing file"),
                current_index: 0,
            }),
            Ok(None) => Err(LoadingError::FileNotFound),
            Err(database_error) => Err(LoadingError::DatabaseError(database_error)),
        }
    }

    /// Read the content of a file from the virtual file system.
    ///
    /// This function does not(!) modify the internal position. In practise, using the Read trait might be more advantageous.
    pub fn random_read<W: Write>(
        &self,
        sink: W,
        index: usize,
        length: usize,
    ) -> Result<usize, ReadError> {
        self.file_system.read(self.handle, sink, index, length)
    }

    /// Query the length of the file.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Checks whether the file is empty.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Query the raw underlying handle.
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// Delete the file from the virtual file system.
    pub fn delete(self) -> bool {
        self.file_system.delete(self.handle) == Ok(1)
    }
}

impl<'a, D: BorrowMut<Database>> Read for File<'a, D> {
    fn read(&mut self, mut buf: &mut [u8]) -> IoResult<usize> {
        let length = std::cmp::min(buf.len(), self.size - self.current_index);
        match self
            .file_system
            .read(self.handle, &mut buf, self.current_index, length)
        {
            Ok(written_bytes) => {
                self.current_index += written_bytes;
                Ok(written_bytes)
            }
            Err(error) => Err(IoError::new(ErrorKind::Other, error.error_message())),
        }
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> IoResult<usize> {
        *buf = vec![0u8; self.size - self.current_index];
        self.read(&mut buf[..])
    }
}

impl<'a, D: BorrowMut<Database>> TryFrom<(&'a FileSystem<D>, Handle)> for File<'a, D> {
    type Error = LoadingError;

    fn try_from(value: (&'a FileSystem<D>, Handle)) -> Result<Self, Self::Error> {
        let (file_system, handle) = value;
        match file_system.size(handle) {
            Ok(Some(size)) => Ok(File {
                file_system,
                handle,
                size,
                current_index: 0,
            }),
            Ok(None) => Err(LoadingError::FileNotFound),
            Err(error) => Err(LoadingError::DatabaseError(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use test_case::test_case;

    use super::super::errors::{CreationError, LoadingError, ReadError};
    use super::{Database, File, FileSystem, FileSystemError, Handle};
    use std::io::Read;

    #[test]
    fn test_loading() {
        let mut connection = Database::open_in_memory().expect("Open in-memory database failed");
        {
            assert_eq!(
                FileSystem::load(&mut connection, false).unwrap_err(),
                FileSystemError::NoFileSystem
            );
        }
        {
            FileSystem::load(&mut connection, true).expect("Creating filesystem failed");
        }
        {
            FileSystem::load(&mut connection, false).expect("Loading created filesystem failed");
        }
    }

    #[test_case(0, 0, 0, 0, false; "File size: 0, Chunk size: 0, First index: 0, Length: 0")]
    #[test_case(1, 0, 0, 1, false; "File size: 1, Chunk size: 0, First index: 0, Length: 1")]
    #[test_case(3, 0, 0, 3, false; "File size: 3, Chunk size: 0, First index: 0, Length: 3")]
    #[test_case(0, 1, 0, 0, false; "File size: 0, Chunk size: 1, First index: 0, Length: 0")]
    #[test_case(1, 1, 0, 1, false; "File size: 1, Chunk size: 1, First index: 0, Length: 1")]
    #[test_case(3, 1, 0, 3, false; "File size: 3, Chunk size: 1, First index: 0, Length: 3")]
    #[test_case(0, 3, 0, 0, false; "File size: 0, Chunk size: 3, First index: 0, Length: 0")]
    #[test_case(1, 3, 0, 1, false; "File size: 1, Chunk size: 3, First index: 0, Length: 1")]
    #[test_case(3, 3, 0, 3, false; "File size: 3, Chunk size: 3, First index: 0, Length: 3")]
    #[test_case(0, 4, 0, 0, false; "File size: 0, Chunk size: 4, First index: 0, Length: 0")]
    #[test_case(1, 4, 0, 1, false; "File size: 1, Chunk size: 4, First index: 0, Length: 1")]
    #[test_case(3, 4, 0, 3, false; "File size: 3, Chunk size: 4, First index: 0, Length: 3")]
    // Test random reads
    #[test_case(3, 0, 1, 2, false; "File size: 3, Chunk size: 0, First index: 1, Length: 2")]
    #[test_case(3, 1, 1, 2, false; "File size: 3, Chunk size: 1, First index: 1, Length: 2")]
    #[test_case(3, 3, 1, 2, false; "File size: 3, Chunk size: 3, First index: 1, Length: 2")]
    #[test_case(3, 4, 1, 2, false; "File size: 3, Chunk size: 4, First index: 1, Length: 2")]
    #[test_case(3, 0, 2, 1, false; "File size: 3, Chunk size: 0, First index: 2, Length: 1")]
    #[test_case(3, 1, 2, 1, false; "File size: 3, Chunk size: 1, First index: 2, Length: 1")]
    #[test_case(3, 3, 2, 1, false; "File size: 3, Chunk size: 3, First index: 2, Length: 1")]
    #[test_case(3, 4, 2, 1, false; "File size: 3, Chunk size: 4, First index: 2, Length: 1")]
    #[test_case(6, 4, 2, 1, false; "File size: 4, Chunk size: 4, First index: 2, Length: 2")]
    // Test out-of-bounds
    #[test_case(0, 0, 0, 1, true; "File size: 0, Chunk size: 0, First index: 0, Length: 1 --> OUT OF BOUNDS!")]
    #[test_case(1, 0, 1, 1, true; "File size: 1, Chunk size: 0, First index: 1, Length: 1 --> OUT OF BOUNDS!")]
    #[test_case(1, 0, 1, 2, true; "File size: 1, Chunk size: 0, First index: 1, Length: 2 --> OUT OF BOUNDS!")]
    #[test_case(3, 0, 1, 3, true; "File size: 3, Chunk size: 0, First index: 1, Length: 3 --> OUT OF BOUNDS!")]
    #[test_case(3, 0, 2, 2, true; "File size: 3, Chunk size: 0, First index: 2, Length: 2 --> OUT OF BOUNDS!")]
    #[test_case(0, 1, 0, 1, true; "File size: 0, Chunk size: 1, First index: 0, Length: 1 --> OUT OF BOUNDS!")]
    #[test_case(1, 1, 1, 1, true; "File size: 1, Chunk size: 1, First index: 1, Length: 1 --> OUT OF BOUNDS!")]
    #[test_case(1, 1, 1, 2, true; "File size: 1, Chunk size: 1, First index: 1, Length: 2 --> OUT OF BOUNDS!")]
    #[test_case(3, 1, 1, 3, true; "File size: 3, Chunk size: 1, First index: 1, Length: 3 --> OUT OF BOUNDS!")]
    #[test_case(3, 1, 2, 2, true; "File size: 3, Chunk size: 1, First index: 2, Length: 2 --> OUT OF BOUNDS!")]
    #[test_case(0, 3, 0, 1, true; "File size: 0, Chunk size: 3, First index: 0, Length: 1 --> OUT OF BOUNDS!")]
    #[test_case(1, 3, 1, 1, true; "File size: 1, Chunk size: 3, First index: 1, Length: 1 --> OUT OF BOUNDS!")]
    #[test_case(1, 3, 1, 2, true; "File size: 1, Chunk size: 3, First index: 1, Length: 2 --> OUT OF BOUNDS!")]
    #[test_case(3, 3, 1, 3, true; "File size: 3, Chunk size: 3, First index: 1, Length: 3 --> OUT OF BOUNDS!")]
    #[test_case(3, 3, 2, 2, true; "File size: 3, Chunk size: 3, First index: 2, Length: 2 --> OUT OF BOUNDS!")]
    #[test_case(0, 4, 0, 1, true; "File size: 0, Chunk size: 4, First index: 0, Length: 1 --> OUT OF BOUNDS!")]
    #[test_case(1, 4, 1, 1, true; "File size: 1, Chunk size: 4, First index: 1, Length: 1 --> OUT OF BOUNDS!")]
    #[test_case(1, 4, 1, 2, true; "File size: 1, Chunk size: 4, First index: 1, Length: 2 --> OUT OF BOUNDS!")]
    #[test_case(3, 4, 1, 3, true; "File size: 3, Chunk size: 4, First index: 1, Length: 3 --> OUT OF BOUNDS!")]
    #[test_case(3, 4, 2, 2, true; "File size: 3, Chunk size: 4, First index: 2, Length: 2 --> OUT OF BOUNDS!")]
    // Special case: It is always save to read data of length 0
    #[test_case(0, 0, 1, 0, false; "File size: 0, Chunk size: 0, First index: 1, Length: 0")]
    #[test_case(0, 1, 1, 0, false; "File size: 0, Chunk size: 1, First index: 1, Length: 0")]
    #[test_case(0, 3, 1, 0, false; "File size: 0, Chunk size: 3, First index: 1, Length: 0")]
    #[test_case(0, 4, 1, 0, false; "File size: 0, Chunk size: 4, First index: 1, Length: 0")]
    fn test_file_handling(
        file_size: u8,
        chunk_size: usize,
        index: usize,
        length: usize,
        is_out_of_bounds: bool,
    ) {
        let data: Vec<_> = (0..file_size).into_iter().collect();
        let path = "file";
        let mut connection = Database::open_in_memory().expect("Open in-memory database failed");
        let mut file_system =
            FileSystem::load(&mut connection, true).expect("Creating filesystem failed");

        // Create file
        {
            let file = File::create(&mut file_system, path, &data[..], chunk_size)
                .expect("Creating file failed");
            assert_eq!(file.len(), data.len());
        }

        // Check that the file could not be overwritten
        assert_eq!(
            File::create(&mut file_system, path, &data[..], chunk_size)
                .expect_err("Able to write file a second time"),
            CreationError::FileExists
        );

        // Load and read file
        {
            let file = File::load(&mut file_system, path).expect("Loading file failed");
            assert_eq!(file.len(), data.len());

            let mut read_data = Vec::new();
            if is_out_of_bounds {
                assert_eq!(
                    file.random_read(&mut read_data, index, length)
                        .expect_err("Reading file content was successful despite out of bounds"),
                    ReadError::OutOfBounds
                );
            } else {
                assert_eq!(
                    file.random_read(&mut read_data, index, length)
                        .expect("Reading file content failed"),
                    length
                );
                assert_eq!(read_data.len(), length);
                if length > 0 {
                    assert_eq!(&read_data, &data[index..(index + length)]);
                }
            }
        }
    }

    #[test]
    fn test_read_trait() {
        let mut file_system = FileSystem::load(
            Database::open_in_memory().expect("Open in-memory database failed"),
            true,
        )
        .expect("Creating filesystem failed");

        let data = [1u8, 2, 3, 4, 5];

        let mut file =
            File::create(&mut file_system, "file", &data[..], 3).expect("File creation failed");
        let mut buffer = vec![0u8; 3];
        assert_eq!(file.read(&mut buffer[..]).expect("Successful read"), 3);
        assert_eq!(&buffer, &[1u8, 2, 3]);

        assert_eq!(file.read_to_end(&mut buffer).expect("Successful read"), 2);
        assert_eq!(&buffer, &[4, 5]);

        // Test that it is safe to read at EOF
        assert_eq!(file.read(&mut buffer[..]).expect("Successful read"), 0);
        assert_eq!(file.read_to_end(&mut buffer).expect("Successful read"), 0);
    }

    #[test]
    fn test_handle() {
        let mut file_system = FileSystem::load(
            Database::open_in_memory().expect("Open in-memory database failed"),
            true,
        )
        .expect("Creating filesystem failed");
        let data = [1u8, 2, 3];

        let handle = {
            let file =
                File::create(&mut file_system, "file", &data[..], 3).expect("File creation failed");
            assert_eq!(file.len(), data.len());
            file.handle
        };

        // Create an invalid handle and check it is not equal to the "real" one
        let invalid_handle: Handle = 42.into();
        assert_ne!(handle, invalid_handle);

        // Re-open file from handle
        {
            let file: File<_> = (&file_system, handle)
                .try_into()
                .expect("Reconstructing file from handle failed");
            assert_eq!(file.len(), data.len());
        }

        // Check that invalid handle is correctly identified
        let invalid_file: Result<File<_>, _> = (&file_system, invalid_handle).try_into();
        assert_eq!(
            invalid_file.expect_err("Successful reconstruction of invalid handle"),
            LoadingError::FileNotFound
        );
    }

    #[test]
    fn test_empty_file() {
        let mut file_system = FileSystem::load(
            Database::open_in_memory().expect("Open in-memory database failed"),
            true,
        )
        .expect("Unable to create file system");
        let data = Vec::new();

        let handle = {
            let file =
                File::create(&mut file_system, "abc", &data[..], 3).expect("Unable to create file");
            assert_eq!(file.len(), 0);
            assert_eq!(file.is_empty(), true);
            file.handle()
        };

        let reopened_file: File<_> = (&file_system, handle)
            .try_into()
            .expect("Unable to re-open empty file");
        assert_eq!(reopened_file.len(), 0);
    }

    #[test]
    fn test_delete() {
        let mut file_system = FileSystem::load(
            Database::open_in_memory().expect("Open in-memory database failed"),
            true,
        )
        .expect("Creating filesystem failed");
        let data = [1u8, 2, 3];
        let path = "abc";

        // Create file
        File::create(&mut file_system, path, &data[..], 3).expect("File creation failed");

        // Check that the file exists
        File::create(&mut file_system, path, &data[..], 3)
            .expect_err("File created despite existent");

        // Delete the file
        let file = File::load(&mut file_system, path).expect("Existing file not found");
        assert!(file.delete());

        // Check the file does not longer exists
        assert_eq!(
            File::load(&mut file_system, path).expect_err("Delete file still found"),
            LoadingError::FileNotFound
        );

        // Check a new file can be created
        File::create(&mut file_system, path, &data[..], 3).expect("File (re-)creation failed");
    }

    #[test]
    fn test_find() {
        let mut file_system = FileSystem::load(
            Database::open_in_memory().expect("Open in-memory database failed"),
            true,
        )
        .expect("Creating filesystem failed");

        let paths = [
            "folder/example_file_1.txt",
            "folder/example_file_2.txt",
            "folder/nested_folder1/file1.txt",
            "folder/nested_folder1/file2.txt",
            "folder/nested_folder2/file1.txt",
        ];
        let data = [1u8, 2, 3];
        for path in paths.iter() {
            File::create(&mut file_system, path, &data[..], 42).expect("Creating file failed");
        }

        // Check non-existing paths
        assert_eq!(file_system.find("folder").expect("Finding failed").len(), 0);

        // Check existing paths - makes no real sense, but...
        assert_eq!(file_system.find(paths[0]).expect("Finding failed").len(), 1);

        // Check single char wildcard
        assert_eq!(
            file_system
                .find("folder/example_file_?.txt")
                .expect("Finding failed")
                .len(),
            2
        );

        // Check multiple char wildcard
        assert_eq!(
            file_system
                .find("folder/example_*.txt")
                .expect("Finding failed")
                .len(),
            2
        );

        // Check multiple char wildcard in folders
        assert_eq!(
            file_system
                .find("folder/*/*")
                .expect("Finding failed")
                .len(),
            3
        );

        // Check general wildcard
        assert_eq!(file_system.find("*").expect("Finding failed").len(), 5);
    }
}
