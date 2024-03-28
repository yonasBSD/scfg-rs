use crate::Directive;
use crate::Scfg;
use std::fmt;
use std::io;

#[derive(Debug)]
enum ErrorKind {
    UnexpectedClosingBrace,
    Io(io::Error),
    ShellWords(shell_words::ParseError),
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    lineno: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parsing error at line {}: ", self.lineno)?;
        match &self.kind {
            ErrorKind::UnexpectedClosingBrace => write!(f, "unexpected '}}'"),
            ErrorKind::Io(err) => write!(f, "io: {}", err),
            ErrorKind::ShellWords(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ErrorKind::Io(err) => Some(err),
            ErrorKind::ShellWords(err) => Some(err),
            _ => None,
        }
    }
}

pub fn document(mut r: impl io::BufRead) -> Result<Scfg, Error> {
    let mut lineno = 0;
    let (block, closing_brace) = read_block(&mut r, &mut lineno)?;
    if closing_brace {
        return Err(Error {
            kind: ErrorKind::UnexpectedClosingBrace,
            lineno,
        });
    }
    Ok(block)
}

/// Reads a block.
///
/// Returns `(block, closing_brace)` where `closing_brace` is true if parsing stopped on '}', and
/// false if parsing stopped on EOF.
///
/// `lineno` must be set the line number of the first line of the block minus one, and is set to
/// the line number of the closing bracket or EOF.
fn read_block<R: io::BufRead>(r: &mut R, lineno: &mut usize) -> Result<(Scfg, bool), Error> {
    let mut block = Scfg::new();
    let mut line = String::new();

    loop {
        *lineno += 1;
        line.clear();
        let n = r.read_line(&mut line).map_err(|err| Error {
            kind: ErrorKind::Io(err),
            lineno: *lineno,
        })?;
        if n == 0 {
            // reached EOF.
            return Ok((block, false));
        }
        let line = line.trim();

        let mut words = shell_words::split(&line).map_err(|err| Error {
            kind: ErrorKind::ShellWords(err),
            lineno: *lineno,
        })?;
        if words.is_empty() {
            // line is either empty or a comment.
            continue;
        }

        let last_byte = *line.as_bytes().last().unwrap();
        if words.len() == 1 && last_byte == b'}' {
            // The line is a litteral '}' (end of block).
            return Ok((block, true));
        }

        let has_child = words.last().unwrap() == "{" && last_byte == b'{'; // avoid matching `"{"`
        let (name, directive) = if has_child {
            words.pop(); // remove brace
            let name = if words.is_empty() {
                String::new()
            } else {
                words.remove(0)
            };
            let (child, closing_brace) = read_block(r, lineno)?;
            if !closing_brace {
                return Err(Error {
                    kind: ErrorKind::Io(io::ErrorKind::UnexpectedEof.into()),
                    lineno: *lineno,
                });
            }
            (
                name,
                Directive {
                    params: words,
                    child: Some(child),
                },
            )
        } else {
            let name = words.remove(0);
            (
                name,
                Directive {
                    params: words,
                    child: None,
                },
            )
        };
        block.add_directive(name, directive);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::*;

    #[test]
    fn unexpected_bracket() {
        let src = r#"domain example.com

# TLS endpoint
listen 0.0.0.0:6697 {
    certificate "/etc/letsencrypt/live/example.com/fullchain.pem"
    key         "/etc/letsencrypt/live/example.com/privkey.pem"
}
}

listen 127.0.0.1:6667
"#;

        let err = Scfg::from_str(src).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedClosingBrace));
        assert_eq!(err.lineno, 8);
    }

    #[test]
    fn unexpected_eof() {
        let src = r#"domain example.com

# TLS endpoint
listen 0.0.0.0:6697 {
    certificate "/etc/letsencrypt/live/example.com/fullchain.pem"
"#;

        let err = Scfg::from_str(src).unwrap_err();
        match err.kind {
            ErrorKind::Io(err) => {
                assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
            }
            _ => {
                panic!("unexpected error kind {:?}", err.kind);
            }
        }
        assert_eq!(err.lineno, 6);
    }

    #[test]
    fn missing_quote() {
        let src = r#"domain example.com

# TLS endpoint
listen 0.0.0.0:6697 {
    certificate "/etc/letsencrypt/live/example.com/fullchain.pem
    key         "/etc/letsencrypt/live/example.com/privkey.pem"
}

listen 127.0.0.1:6667
"#;

        let err = Scfg::from_str(src).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::ShellWords(_)));
        assert_eq!(err.lineno, 5);
    }
}
