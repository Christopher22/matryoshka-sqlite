use const_format::concatcp;
use regex::Regex;
use rusqlite::Connection as Database;
use rusqlite::Error as DatabaseError;

/// Meta data associated with the virtual file system.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct MetaData(u32);

/// The availability of the file system in a SQLite database.
#[derive(Debug, PartialEq)]
pub enum Availability {
    /// There is a file system available.
    Available(MetaData),
    /// There is no file system available.
    Missing,
    /// During querying, there was an SQLite error.
    Error(DatabaseError),
}

impl MetaData {
    /// Create a meta data directly for a specific version.
    pub const fn from_version(version: u32) -> Self {
        MetaData(version)
    }

    /// Queries a database for the most recent meta data available.
    pub fn from_database(database: &Database) -> Availability {
        let mut statement = match database
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE ?")
        {
            Ok(statement) => statement,
            Err(err) => return Availability::Error(err),
        };

        let versions =
            match statement.query(&[concatcp!(MetaDataExtractor::META_TABLE_PREFIX, "%")]) {
                Ok(versions) => versions,
                Err(err) => return Availability::Error(err),
            };

        let version_extractor = MetaDataExtractor::default();
        let last_version = versions
            .mapped(|row| {
                match row
                    .get_ref(0)
                    .map(|value| value.as_str().expect("Table name is not a string"))
                {
                    Ok(text) => Ok(version_extractor.extract(text)),
                    Err(error) => Err(error),
                }
            })
            .filter_map(|value| match value {
                Ok(Some(version)) => Some(MetaData(version)),
                _ => None,
            })
            .max();

        match last_version {
            Some(meta_data) => Availability::Available(meta_data),
            None => Availability::Missing,
        }
    }

    /// Returns the version of the file system.
    pub fn version(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone)]
/// An internal extractor for the version of Matryoshka.
struct MetaDataExtractor(Regex);

impl Default for MetaDataExtractor {
    fn default() -> Self {
        MetaDataExtractor(
            Regex::new(concatcp!(MetaDataExtractor::META_TABLE_PREFIX, "([0-9]+)"))
                .expect("Encounter invalid Matryoshka RegEx"),
        )
    }
}

impl MetaDataExtractor {
    const META_TABLE_PREFIX: &'static str = "Matryoshka_Meta_";

    #[cfg(test)]
    pub fn generate_table_name(version: u32) -> String {
        format!("{}{}", MetaDataExtractor::META_TABLE_PREFIX, version)
    }

    pub fn extract<T: AsRef<str>>(&self, value: T) -> Option<u32> {
        self.0.captures(value.as_ref()).and_then(|value| {
            value
                .get(1)
                .and_then(|value| str::parse(value.as_str()).ok())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Availability, Database, MetaData, MetaDataExtractor};

    #[test]
    fn test_extractor() {
        let version_extractor = MetaDataExtractor::default();
        assert_eq!(
            &MetaDataExtractor::generate_table_name(0),
            "Matryoshka_Meta_0"
        );
        assert_eq!(
            version_extractor.extract(MetaDataExtractor::generate_table_name(0)),
            Some(0)
        );
        assert_eq!(
            &MetaDataExtractor::generate_table_name(1),
            "Matryoshka_Meta_1"
        );
        assert_eq!(
            version_extractor.extract(MetaDataExtractor::generate_table_name(1)),
            Some(1)
        );
        assert_eq!(
            &MetaDataExtractor::generate_table_name(42),
            "Matryoshka_Meta_42"
        );
        assert_eq!(
            version_extractor.extract(MetaDataExtractor::generate_table_name(42)),
            Some(42)
        );
    }

    #[test]
    fn test_missing_filesystem() {
        let database = Database::open_in_memory().expect("Valid SQLite database");
        assert_eq!(MetaData::from_database(&database), Availability::Missing);
    }

    #[test]
    fn test_existing_filesystem() {
        let database = Database::open_in_memory().expect("Valid SQLite database");
        database
            .execute(
                &format!(
                    "CREATE TABLE {} (example TEXT)",
                    MetaDataExtractor::generate_table_name(0)
                ),
                [],
            )
            .expect("Create database failed");
        assert_eq!(
            MetaData::from_database(&database),
            Availability::Available(MetaData(0))
        );

        // The most recent meta data is discovered.
        database
            .execute(
                &format!(
                    "CREATE TABLE {} (example TEXT)",
                    MetaDataExtractor::generate_table_name(42)
                ),
                [],
            )
            .expect("Create database failed");
        assert_eq!(
            MetaData::from_database(&database),
            Availability::Available(MetaData(42))
        );
    }
}
