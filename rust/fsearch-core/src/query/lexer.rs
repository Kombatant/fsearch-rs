use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Eos,
    Equal,
    Contains,
    Smaller,
    SmallerEq,
    Greater,
    GreaterEq,
    Not,
    BracketOpen,
    BracketClose,
    Field(String),
    FieldEmpty(String),
    Word(String),
    And,
    Or,
}

pub struct Lexer {
    input: String,
    pos: usize,
    pushback: VecDeque<char>,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            input: input.to_string(),
            pos: 0,
            pushback: VecDeque::new(),
        }
    }

    fn get_next_input_char(&mut self) -> Option<char> {
        if self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next().unwrap();
            self.pos += ch.len_utf8();
            Some(ch)
        } else {
            None
        }
    }

    fn get_next_char(&mut self) -> Option<char> {
        if let Some(c) = self.pushback.pop_front() {
            Some(c)
        } else {
            self.get_next_input_char()
        }
    }

    fn give_back_char(&mut self, c: char) {
        self.pushback.push_front(c);
    }

    fn parse_quoted_string(&mut self) -> String {
        let mut out = String::new();
        while let Some(c) = self.get_next_char() {
            if c == '"' {
                break;
            } else {
                out.push(c);
            }
        }
        out
    }

    pub fn next_token(&mut self) -> (Token, Option<String>) {
        // skip whitespace
        loop {
            match self.get_next_char() {
                Some(c) if c.is_whitespace() => continue,
                Some(c) => {
                    // process
                    match c {
                        '\0' => return (Token::Eos, None),
                        '=' => return (Token::Equal, None),
                        ':' => return (Token::Contains, None),
                        '<' => {
                            if let Some('=') = self.get_next_char() {
                                return (Token::SmallerEq, None);
                            } else {
                                return (Token::Smaller, None);
                            }
                        }
                        '>' => {
                            if let Some('=') = self.get_next_char() {
                                return (Token::GreaterEq, None);
                            } else {
                                return (Token::Greater, None);
                            }
                        }
                        '!' => return (Token::Not, None),
                        '(' => return (Token::BracketOpen, None),
                        ')' => return (Token::BracketClose, None),
                        '"' => {
                            let s = self.parse_quoted_string();
                            return (Token::Word(s.clone()), Some(s));
                        }
                        '\\' => {
                            if let Some(next) = self.get_next_char() {
                                let mut s = String::new();
                                s.push(next);
                                return (Token::Word(s.clone()), Some(s));
                            }
                        }
                        other => {
                            // start reading token until whitespace or reserved char
                            let mut s = String::new();
                            s.push(other);
                            while let Some(nc) = self.get_next_char() {
                                if nc.is_whitespace() || ":=<>()\"\\".contains(nc) {
                                    self.give_back_char(nc);
                                    break;
                                }
                                s.push(nc);
                            }
                            // reserved words
                            if s == "NOT" {
                                return (Token::Not, None);
                            }
                            if s == "AND" || s == "&&" {
                                return (Token::And, None);
                            }
                            if s == "OR" || s == "||" {
                                return (Token::Or, None);
                            }
                            // if next is ':' then it's a field
                            if let Some(next_c) = self.get_next_char() {
                                if next_c == ':' {
                                    // check if next is whitespace or eos
                                    if let Some(peek) = self.get_next_char() {
                                        if peek.is_whitespace() {
                                            return (Token::FieldEmpty(s.clone()), Some(s));
                                        } else {
                                            self.give_back_char(peek);
                                            return (Token::Field(s.clone()), Some(s));
                                        }
                                    } else {
                                        return (Token::FieldEmpty(s.clone()), Some(s));
                                    }
                                } else {
                                    self.give_back_char(next_c);
                                }
                            }
                            return (Token::Word(s.clone()), Some(s));
                        }
                    }
                }
                None => return (Token::Eos, None),
            }
        }
    }

    pub fn peek_token(&mut self) -> (Token, Option<String>) {
        // save state
        let old_pos = self.pos;
        let old_push = self.pushback.clone();
        let res = self.next_token();
        // restore
        self.pos = old_pos;
        self.pushback = old_push;
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexer_basic_tokens() {
        let mut lx = Lexer::new("name:foo AND bar OR (baz)");
        let (t1, _) = lx.next_token();
        assert_eq!(t1, Token::Field("name".into()));
        let (t2, _) = lx.next_token();
        assert_eq!(t2, Token::Word("foo".into()));
        let (t3, _) = lx.next_token();
        assert_eq!(t3, Token::And);
        let (t4, _) = lx.next_token();
        assert_eq!(t4, Token::Word("bar".into()));
        let (t5, _) = lx.next_token();
        assert_eq!(t5, Token::Or);
        let (t6, _) = lx.next_token();
        assert_eq!(t6, Token::BracketOpen);
        let (t7, _) = lx.next_token();
        assert_eq!(t7, Token::Word("baz".into()));
        let (t8, _) = lx.next_token();
        assert_eq!(t8, Token::BracketClose);
    }
}
