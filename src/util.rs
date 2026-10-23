pub fn write_comma_separated(
    f: &mut std::fmt::Formatter<'_>,
    items: impl IntoIterator<Item = impl std::fmt::Display>,
) -> Result<(), std::fmt::Error> {
    Ok(for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        item.fmt(f)?;
    })
}

#[macro_export]
macro_rules! usize_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(usize);

        impl $name {
            pub fn get(self) -> usize {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
