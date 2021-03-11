/// A raw handle to a file in the virtual file system.
#[derive(Clone, Copy, Debug, PartialOrd, PartialEq, Eq, Ord)]
pub struct Handle(pub i64);

impl From<i64> for Handle {
    fn from(raw_value: i64) -> Self {
        Handle(raw_value)
    }
}
