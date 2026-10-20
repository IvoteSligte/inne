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
