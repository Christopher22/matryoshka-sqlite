use std::path::{Component, Path};

#[derive(Debug, Clone, PartialOrd, Ord)]
/// A virtual path. Unlike a path file system, it is always properly normalized and valid Unicode.
pub struct VirtualPath(String);

impl VirtualPath {
    /// Generates a virtual path from an given arguments. By default, simply use the "Into" trait.
    pub fn from<T: AsRef<Path>>(value: T) -> Self {
        value.into()
    }
}

impl<T> From<T> for VirtualPath
where
    T: AsRef<Path>,
{
    fn from(path: T) -> Self {
        // Pre-filter components to get an size estimate
        let components: Vec<_> = path
            .as_ref()
            .components()
            .filter_map(|component| match component {
                Component::Normal(raw_path) => match raw_path.to_str() {
                    Some(value) => Some(Some(value.to_string())),
                    None => None,
                },
                Component::ParentDir => Some(None),
                _ => None,
            })
            .collect();

        // Remove parents inside
        let mut parts = Vec::with_capacity(components.len());
        for component in components {
            match component {
                Some(value) => {
                    parts.push(value);
                }
                None => {
                    parts.pop();
                }
            }
        }

        VirtualPath(parts.join("/"))
    }
}

impl AsRef<str> for VirtualPath {
    fn as_ref(&self) -> &str {
        return self.0.as_str();
    }
}

impl<T> PartialEq<T> for VirtualPath
where
    T: AsRef<str>,
{
    fn eq(&self, other: &T) -> bool {
        self.0.as_str() == other.as_ref()
    }
}

impl Eq for VirtualPath {}

#[cfg(test)]
mod tests {
    use super::VirtualPath;

    #[test]
    fn test_special() {
        assert_eq!(VirtualPath::from("/"), "");
        assert_eq!(VirtualPath::from("."), "");
        assert_eq!(VirtualPath::from(".."), "");
    }

    #[test]
    fn test_multiple() {
        assert_eq!(VirtualPath::from("42"), "42");
        assert_eq!(VirtualPath::from("/42"), "42");
        assert_eq!(VirtualPath::from("42/"), "42");
        assert_eq!(VirtualPath::from("/42/"), "42");
    }

    #[test]
    fn test_multiple_parts() {
        assert_eq!(VirtualPath::from("42/PI"), "42/PI");
        assert_eq!(VirtualPath::from("/42/PI/"), "42/PI");
        assert_eq!(VirtualPath::from("/42/PI"), "42/PI");
    }

    #[test]
    fn test_current_dir() {
        assert_eq!(VirtualPath::from("/42/."), "42");
        assert_eq!(VirtualPath::from("/42/./"), "42");
        assert_eq!(VirtualPath::from("/42/./PI"), "42/PI");
    }

    #[test]
    fn test_parent_dir() {
        assert_eq!(VirtualPath::from("/42/.."), "");
        assert_eq!(VirtualPath::from("42/.."), "");
        assert_eq!(VirtualPath::from("./.."), "");
        assert_eq!(VirtualPath::from("42/../"), "");
        assert_eq!(VirtualPath::from("42/../PI"), "PI");
        assert_eq!(VirtualPath::from("42/./../PI/"), "PI");
        assert_eq!(VirtualPath::from("42/43/../PI/"), "42/PI");
    }
}
