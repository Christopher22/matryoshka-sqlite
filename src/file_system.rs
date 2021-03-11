use std::convert::TryInto;
use std::io::{ErrorKind, Read};

use const_format::formatcp;
use rusqlite::limits::Limit;
use rusqlite::{params, Connection as Database, Error, ErrorCode, OptionalExtension};

use super::errors::{CreationError, DatabaseError, FileSystemError, LoadingError};
use super::util::{Availability, Handle, MetaData, VirtualPath};

/// A virtual file system in a SQLite database.
#[derive(Debug)]
pub struct FileSystem<'a> {
    /// A reference to the SQLite database.
    pub database: &'a mut Database,
    meta_data: MetaData,
}

/// A file stored in the virtual file system.
#[derive(Debug)]
pub struct File<'a, 'fs> {
    file_system: &'a FileSystem<'fs>,
    handle: Handle,
}

impl<'a> FileSystem<'a> {
    const CURRENT_MATRYOSHKA_VERSION: u32 = 0;
    const MATRYOSHKA_TABLE: &'static str = "Matryoshka_Meta_0";
    const DATA_TABLE: &'static str = "Matryoshka_Data";

    const DEFAULT_BYTE_BLOB_SIZE: usize = 33554432; // 32MB

    const SQL_CREATE_META: &'static str = formatcp!(
        "CREATE TABLE {} (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, type INTEGER, flags INTEGER, chunk_size INTEGER NOT NULL)",
        FileSystem::MATRYOSHKA_TABLE
    );
    const SQL_CREATE_DATA: &'static str = formatcp!(
        "CREATE TABLE IF NOT EXISTS {} (chunk_id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL, chunk_num INTEGER NOT NULL, data BLOB NOT NULL, CONSTRAINT unq UNIQUE (file_id, chunk_num), FOREIGN KEY(file_id) REFERENCES {} (id) ON DELETE CASCADE ON UPDATE CASCADE)",
        FileSystem::DATA_TABLE,
        FileSystem::MATRYOSHKA_TABLE
    );
    const SQL_CREATE_HANDLE: &'static str = formatcp!(
        "INSERT INTO {} (path, type, chunk_size) VALUES (?, ?, ?)",
        FileSystem::MATRYOSHKA_TABLE
    );
    const SQL_CREATE_BLOB: &'static str = formatcp!(
        "INSERT INTO {} (file_id, chunk_num, data) VALUES (?, ?, ?)",
        FileSystem::DATA_TABLE
    );
    const SQL_GET_HANDLE: &'static str = formatcp!(
        "SELECT id FROM {} WHERE path = ? AND type = ?",
        FileSystem::MATRYOSHKA_TABLE
    );
    const SQL_GLOB: &'static str = formatcp!(
        "SELECT path FROM {} WHERE path GLOB ? AND type = ?",
        FileSystem::MATRYOSHKA_TABLE
    );
    const SQL_SIZE: &'static str = formatcp!(
        "SELECT COALESCE(SUM(LENGTH(data)), 0) FROM {} WHERE file_id = ?",
        FileSystem::DATA_TABLE
    );
    const SQL_DELETE: &'static str =
        formatcp!("DELETE FROM {} WHERE id = ?", FileSystem::MATRYOSHKA_TABLE);
    const SQL_GET_BLOBS: &'static str = formatcp!("SELECT chunk_id, chunk_num, {meta}.chunk_size FROM {data}
        INNER JOIN {meta} ON {meta}.id={data}.file_id
        WHERE file_id = :handle AND chunk_num BETWEEN cast((:index / {meta}.chunk_size) as int) AND cast(((:index + :size - 1) / {meta}.chunk_size) as int)
        ORDER BY chunk_num ASC",
        data=FileSystem::DATA_TABLE,
        meta=FileSystem::MATRYOSHKA_TABLE
    );

    /// Load the virtual file system from an SQLite database.
    pub fn load(
        database: &mut Database,
        create_file_system: bool,
    ) -> Result<FileSystem, FileSystemError> {
        let meta_data = match MetaData::from_database(&database) {
            Availability::Available(meta_data)
                if meta_data.version() == FileSystem::CURRENT_MATRYOSHKA_VERSION =>
            {
                Ok(meta_data)
            }
            Availability::Available(meta_data) => {
                Err(FileSystemError::UnsupportedVersion(meta_data.version()))
            }
            Availability::Missing if create_file_system => {
                let transaction = database.transaction()?;
                transaction.execute(FileSystem::SQL_CREATE_META, rusqlite::NO_PARAMS)?;
                transaction.execute(FileSystem::SQL_CREATE_DATA, rusqlite::NO_PARAMS)?;
                transaction.commit()?;
                Ok(MetaData::from_version(
                    FileSystem::CURRENT_MATRYOSHKA_VERSION,
                ))
            }
            Availability::Missing => Err(FileSystemError::NoFileSystem),
            Availability::Error(error) => Err(error.into()),
        }?;

        // Pre-compile the primary SQL commands
        const PRECOMPILED_COMMANDS: [&'static str; 6] = [
            FileSystem::SQL_GET_HANDLE,
            FileSystem::SQL_CREATE_HANDLE,
            FileSystem::SQL_GLOB,
            FileSystem::SQL_SIZE,
            FileSystem::SQL_DELETE,
            FileSystem::SQL_GET_BLOBS,
        ];

        database.set_prepared_statement_cache_capacity(PRECOMPILED_COMMANDS.len());
        for statement in &PRECOMPILED_COMMANDS {
            database
                .prepare_cached(statement)
                .or_else(|error| Err(FileSystemError::InvalidBaseCommand(statement, error)))?;
        }

        Ok(FileSystem {
            database,
            meta_data,
        })
    }

    fn create<T: Into<VirtualPath>, R: Read>(
        &mut self,
        path: T,
        mut data: R,
        chunk_size: usize,
    ) -> Result<Handle, CreationError> {
        let max_blob_size = self.database.limit(Limit::SQLITE_LIMIT_LENGTH);
        let chunk_size = match chunk_size {
            value if value > 0 && value <= max_blob_size as usize => value,
            _ => FileSystem::DEFAULT_BYTE_BLOB_SIZE,
        };

        // Create the transaction to return safely on errors and prepare the statement.
        let transaction = self.database.transaction()?;

        let handle = {
            let mut create_handle_statement =
                transaction.prepare_cached(FileSystem::SQL_CREATE_HANDLE)?;
            let mut create_blob_statement =
                transaction.prepare_cached(FileSystem::SQL_CREATE_BLOB)?;

            let handle = match create_handle_statement.insert(params![
                path.into().as_ref(),
                File::TYPE_ID,
                chunk_size as i32
            ]) {
                Ok(handle) => handle,
                Err(Error::SqliteFailure(error, _))
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
            .prepare_cached(FileSystem::SQL_GET_HANDLE)
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))?;
        handle_query
            .query_row(params![path.into().as_ref(), File::TYPE_ID], |row| {
                Ok(Handle(row.get_unwrap(0)))
            })
            .optional()
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }

    fn size(&self, handle: Handle) -> Result<usize, DatabaseError> {
        let mut handle_query = self
            .database
            .prepare_cached(FileSystem::SQL_SIZE)
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))?;
        handle_query
            .query_row(params![handle.0], |row| {
                Ok(row.get_unwrap::<_, i32>(0) as usize)
            })
            .map_err(|error| error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }
}

impl<'a, 'fs> File<'a, 'fs> {
    const TYPE_ID: u32 = 0;

    /// Create a file in the virtual file system.
    pub fn create<T: AsRef<str>, R: Read>(
        file_system: &'a mut FileSystem<'fs>,
        path: T,
        data: R,
        chunk_size: usize,
    ) -> Result<File<'a, 'fs>, CreationError> {
        file_system
            .create(path.as_ref(), data, chunk_size)
            .map(move |handle| File {
                file_system,
                handle,
            })
    }

    /// Load a file from the virtual file system.
    pub fn load<T: AsRef<str>>(
        file_system: &'a FileSystem<'fs>,
        path: T,
    ) -> Result<File<'a, 'fs>, LoadingError> {
        match file_system.open(path.as_ref()) {
            Ok(Some(handle)) => Ok(File {
                file_system,
                handle,
            }),
            Ok(None) => Err(LoadingError::FileNotFound),
            Err(database_error) => Err(LoadingError::DatabaseError(database_error)),
        }
    }

    /// Query the length of the file.
    pub fn len(&self) -> usize {
        self.file_system.size(self.handle).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use crate::errors::CreationError;

    use super::{Database, File, FileSystem, FileSystemError};

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

    #[test_case("file", 0, 0; "'file', File size: 0, Chunk size: 0")]
    #[test_case("file", 1, 0; "'file', File size: 1, Chunk size: 0")]
    #[test_case("file", 3, 0; "'file', File size: 3, Chunk size: 0")]
    #[test_case("file", 0, 1; "'file', File size: 0, Chunk size: 1")]
    #[test_case("file", 1, 1; "'file', File size: 1, Chunk size: 1")]
    #[test_case("file", 3, 1; "'file', File size: 3, Chunk size: 1")]
    #[test_case("file", 0, 3; "'file', File size: 0, Chunk size: 3")]
    #[test_case("file", 1, 3; "'file', File size: 1, Chunk size: 3")]
    #[test_case("file", 3, 3; "'file', File size: 3, Chunk size: 3")]
    #[test_case("file", 0, 4; "'file', File size: 0, Chunk size: 4")]
    #[test_case("file", 1, 4; "'file', File size: 1, Chunk size: 4")]
    #[test_case("file", 3, 4; "'file', File size: 3, Chunk size: 4")]
    fn create_file(path: &str, file_size: u8, chunk_size: usize) {
        let data: Vec<_> = (0..file_size).into_iter().collect();
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

        // Load file
        {
            let file = File::load(&mut file_system, path).expect("Loading file failed");
            assert_eq!(file.len(), data.len());
        }
    }
}
