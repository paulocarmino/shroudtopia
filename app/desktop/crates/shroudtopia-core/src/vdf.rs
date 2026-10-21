//! Valve's text KeyValues (`loginusers.vdf`, `libraryfolders.vdf`, `appmanifest_*.acf`): quoted keys, quoted values
//! or `{ ... }` blocks, `//` comments. Keys keep their order; lookups ignore case, as Steam does.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Text(String),
    Block(Vec<(String, Node)>),
}

impl Node {
    /// The child named `key` (case ignored), when this is a block.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Block(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Node::Text(_) => None,
        }
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Node::Text(t) => Some(t),
            Node::Block(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Node)] {
        match self {
            Node::Block(items) => items,
            Node::Text(_) => &[],
        }
    }
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

impl<'a> Lexer<'a> {
    fn next_token(&mut self) -> Option<Token> {
        loop {
            let c = *self.chars.peek()?;
            if c.is_whitespace() || c == '\u{feff}' {
                self.chars.next();
            } else if c == '/' {
                self.chars.next();
                if self.chars.peek() == Some(&'/') {
                    for c in self.chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
            } else {
                break;
            }
        }
        match self.chars.next()? {
            '{' => Some(Token::Open),
            '}' => Some(Token::Close),
            '"' => {
                let mut s = String::new();
                while let Some(c) = self.chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match self.chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(other) => s.push(other),
                            None => break,
                        },
                        c => s.push(c),
                    }
                }
                Some(Token::Str(s))
            }
            first => {
                // an unquoted token (some files have them): up to whitespace or a brace
                let mut s = String::from(first);
                while let Some(&c) = self.chars.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    self.chars.next();
                }
                Some(Token::Str(s))
            }
        }
    }
}

fn parse_block(lex: &mut Lexer, top: bool) -> Option<Vec<(String, Node)>> {
    let mut items = Vec::new();
    loop {
        match lex.next_token() {
            None => return if top { Some(items) } else { None },
            Some(Token::Close) => return if top { None } else { Some(items) },
            Some(Token::Open) => return None,
            Some(Token::Str(key)) => match lex.next_token()? {
                Token::Str(value) => items.push((key, Node::Text(value))),
                Token::Open => items.push((key, Node::Block(parse_block(lex, false)?))),
                Token::Close => return None,
            },
        }
    }
}

/// The whole file as a block of its top-level keys; None when it is not KeyValues.
pub fn parse(text: &str) -> Option<Node> {
    let mut lex = Lexer { chars: text.chars().peekable() };
    parse_block(&mut lex, true).map(Node::Block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_blocks_escapes_and_unicode() {
        let text = "\"users\"\n{\n\t\"765\"\n\t{\n\t\t\"PersonaName\"\t\t\"[ツ] Ｌ　Ａ\"\n\t\t\"MostRecent\"\t\"1\"\n\t}\n}\n// comment\n";
        let root = parse(text).unwrap();
        let users = root.get("USERS").unwrap();
        assert_eq!(users.entries()[0].0, "765");
        assert_eq!(users.get("765").unwrap().text("personaname"), Some("[ツ] Ｌ　Ａ"));
        let path = parse(r#""a" { "path" "C:\\Program Files (x86)\\Steam" }"#).unwrap();
        assert_eq!(path.get("a").unwrap().text("path"), Some(r"C:\Program Files (x86)\Steam"));
        assert!(parse("\"a\" { \"b\" \"c\"").is_none(), "unclosed");
        assert!(parse("}").is_none());
    }
}
