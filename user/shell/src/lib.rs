// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Parse a bounded shell subset: four stages, quotes/escapes and per-stage redirection.
//! Commands receive one joined argument string under the existing native v1 exec contract.
#[derive(Clone, Copy)]
pub struct Text {
    bytes: [u8; 128],
    len: usize,
}
impl Default for Text {
    fn default() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct ParseError;
impl Text {
    fn push(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.len == self.bytes.len() {
            return Err(ParseError);
        }
        self.bytes[self.len] = byte;
        self.len += 1;
        Ok(())
    }
    pub fn text(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap()
    }
    pub fn empty(&self) -> bool {
        self.len == 0
    }
}
#[derive(Clone, Copy, Default)]
pub struct Command {
    pub name: Text,
    pub argument: Text,
    pub input: Text,
    pub output: Text,
    pub append: bool,
}
pub struct Pipeline {
    pub commands: [Command; 4],
    pub count: usize,
}
#[derive(Clone, Copy, Default)]
struct Token {
    text: Text,
    kind: u8,
}
pub fn parse(line: &[u8]) -> Result<Pipeline, ParseError> {
    if core::str::from_utf8(line).is_err() || line.contains(&0) {
        return Err(ParseError);
    }
    let mut tokens = [Token::default(); 32];
    let mut count = 0;
    let mut at = 0;
    while at < line.len() {
        if line[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if count == tokens.len() {
            return Err(ParseError);
        }
        let token = &mut tokens[count];
        count += 1;
        match line[at] {
            b'|' => {
                token.kind = 1;
                at += 1;
            }
            b'<' => {
                token.kind = 2;
                at += 1;
            }
            b'>' => {
                token.kind = 3;
                at += 1;
                if line.get(at) == Some(&b'>') {
                    token.kind = 4;
                    at += 1;
                }
            }
            _ => {
                let mut quote = 0;
                while at < line.len() {
                    let b = line[at];
                    if quote == 0 && (b.is_ascii_whitespace() || b"|<>".contains(&b)) {
                        break;
                    }
                    at += 1;
                    if b == b'\\' && quote != b'\'' {
                        let c = *line.get(at).ok_or(ParseError)?;
                        token.text.push(c)?;
                        at += 1;
                    } else if (b == b'\'' || b == b'"') && (quote == 0 || quote == b) {
                        quote = if quote == 0 { b } else { 0 };
                    } else {
                        token.text.push(b)?;
                    }
                }
                if quote != 0 || core::str::from_utf8(&token.text.bytes[..token.text.len]).is_err()
                {
                    return Err(ParseError);
                }
            }
        }
    }
    let mut pipeline = Pipeline {
        commands: [Command::default(); 4],
        count: usize::from(count > 0),
    };
    let mut i = 0;
    while i < count {
        let t = tokens[i];
        let c = &mut pipeline.commands[pipeline.count - 1];
        match t.kind {
            0 if c.name.empty() => {
                c.name = t.text;
                if c.name.empty() {
                    return Err(ParseError);
                }
            }
            0 => {
                if !c.argument.empty() {
                    c.argument.push(b' ')?;
                }
                for b in t.text.text().bytes() {
                    c.argument.push(b)?;
                }
            }
            1 => {
                if c.name.empty() || pipeline.count == 4 {
                    return Err(ParseError);
                }
                pipeline.count += 1;
            }
            2..=4 => {
                i += 1;
                let path = tokens.get(i).filter(|_| i < count).ok_or(ParseError)?;
                if path.kind != 0 || path.text.empty() {
                    return Err(ParseError);
                }
                if t.kind == 2 {
                    if !c.input.empty() {
                        return Err(ParseError);
                    }
                    c.input = path.text;
                } else {
                    if !c.output.empty() {
                        return Err(ParseError);
                    }
                    c.output = path.text;
                    c.append = t.kind == 4;
                }
            }
            _ => return Err(ParseError),
        }
        i += 1;
    }
    if count > 0 && pipeline.commands[pipeline.count - 1].name.empty() {
        return Err(ParseError);
    }
    Ok(pipeline)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_quotes_escaped_operators_pipelines_and_redirections() {
        let p = parse(b"echo 'a | b' > /tmp/a | cat <'/tmp/a' >> /tmp/out").unwrap();
        assert_eq!(p.count, 2);
        assert_eq!(p.commands[0].argument.text(), "a | b");
        assert_eq!(p.commands[1].input.text(), "/tmp/a");
        assert!(p.commands[1].append);
        assert_eq!(
            parse(b"echo a\\>b").unwrap().commands[0].argument.text(),
            "a>b"
        );
        assert_eq!(parse(b"   ").unwrap().count, 0);
    }
    #[test]
    fn rejects_bad_syntax_and_bounded_overflow() {
        for line in [
            b"| cat".as_slice(),
            b"echo x |",
            b"cat <",
            b"echo > a > b",
            b"echo 'bad",
            b"a|b|c|d|e",
            b"\0",
        ] {
            assert!(parse(line).is_err());
        }
        assert!(parse(&[b'x'; 129]).is_err());
    }
}
