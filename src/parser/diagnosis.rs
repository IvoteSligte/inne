/// Returns [None] if the line contains only whitespace
fn line_indentation(line: &str) -> Option<usize> {
    let mut indent = 0usize;
    for char in line.chars() {
        match char {
            ' ' => indent += 1,
            '\t' => indent = indent.next_multiple_of(4),
            '\n' => unreachable!(),
            '\r' => {}
            _ => return Some(indent),
        }
    }
    None
}

pub struct MissingRightBrace {
    pub left: usize,
    pub right: usize,
}

impl MissingRightBrace {
    pub fn message(&self) -> &str {
        "This closing brace does not match the indentation of the previous one."
    }

    pub fn diagnose(source: &str) -> Option<Self> {
        let mut left_braces = Vec::new();
        let mut byte_index = 0;

        for line in source.lines() {
            let Some(indent) = line_indentation(line) else {
                byte_index += line.len();
                continue;
            };
            for char in line.chars() {
                if char == '{' {
                    left_braces.push((byte_index, indent));
                } else if char == '}' {
                    if let Some(&(left_byte_index, left_indent)) = left_braces.last()
                        && left_indent != indent
                    {
                        return Some(Self {
                            left: left_byte_index,
                            right: byte_index,
                        });
                    }
                    left_braces.pop();
                }
                byte_index += char.len_utf8();
            }
        }
        None
    }
}
