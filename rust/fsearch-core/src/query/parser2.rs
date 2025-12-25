use crate::query::lexer::{Lexer, Token};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    Not(Box<Node>),
    Word(String),
    Field(String, String),
    Compare(String, CompareOp, String),
    Regex(String),
    Group(Box<Node>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Contains,
    Smaller,
    SmallerEq,
    Greater,
    GreaterEq,
}

pub struct Parser {
    lexer: Lexer,
    cur: Token,
    cur_text: Option<String>,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        let mut lx = Lexer::new(input);
        let (tok, text) = lx.next_token();
        Parser { lexer: lx, cur: tok, cur_text: text }
    }

    fn advance(&mut self) {
        let (tok, text) = self.lexer.next_token();
        self.cur = tok;
        self.cur_text = text;
    }

    pub fn parse(&mut self) -> Option<Node> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<Node> {
        let mut node = self.parse_and()?;
        while let Token::Or = self.cur {
            self.advance();
            let right = self.parse_and()?;
            node = Node::Or(Box::new(node), Box::new(right));
        }
        Some(node)
    }

    fn parse_and(&mut self) -> Option<Node> {
        let mut node = self.parse_unary()?;
        while let Token::And = self.cur {
            self.advance();
            let right = self.parse_unary()?;
            node = Node::And(Box::new(node), Box::new(right));
        }
        Some(node)
    }

    fn parse_unary(&mut self) -> Option<Node> {
        match &self.cur {
            Token::Not => {
                self.advance();
                let inner = self.parse_unary()?;
                Some(Node::Not(Box::new(inner)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<Node> {
        match &self.cur {
            Token::BracketOpen => {
                self.advance();
                let inner = self.parse()?;
                if let Token::BracketClose = self.cur {
                    self.advance();
                }
                Some(Node::Group(Box::new(inner)))
            }
            Token::Field(name) => {
                let field_name = name.clone();
                self.advance();
                match &self.cur {
                    Token::Word(w) => {
                        let term = w.clone();
                        self.advance();
                        Some(Node::Field(field_name, term))
                    }
                    _ => None,
                }
            }
            Token::FieldEmpty(name) => {
                let field_name = name.clone();
                self.advance();
                Some(Node::Field(field_name, String::new()))
            }
            Token::Word(name) => {
                let name_clone = name.clone();
                let (next_tok, _next_text) = self.lexer.peek_token();
                match next_tok {
                    Token::Smaller => {
                        self.advance();
                        self.advance();
                        if let Token::Word(v) = &self.cur {
                            let val = v.clone();
                            self.advance();
                            return Some(Node::Compare(name_clone, CompareOp::Smaller, val));
                        }
                        return None;
                    }
                    Token::SmallerEq => {
                        self.advance();
                        self.advance();
                        if let Token::Word(v) = &self.cur {
                            let val = v.clone();
                            self.advance();
                            return Some(Node::Compare(name_clone, CompareOp::SmallerEq, val));
                        }
                        return None;
                    }
                    Token::Greater => {
                        self.advance();
                        self.advance();
                        if let Token::Word(v) = &self.cur {
                            let val = v.clone();
                            self.advance();
                            return Some(Node::Compare(name_clone, CompareOp::Greater, val));
                        }
                        return None;
                    }
                    Token::GreaterEq => {
                        self.advance();
                        self.advance();
                        if let Token::Word(v) = &self.cur {
                            let val = v.clone();
                            self.advance();
                            return Some(Node::Compare(name_clone, CompareOp::GreaterEq, val));
                        }
                        return None;
                    }
                    Token::Equal => {
                        self.advance();
                        self.advance();
                        if let Token::Word(v) = &self.cur {
                            let val = v.clone();
                            self.advance();
                            return Some(Node::Compare(name_clone, CompareOp::Eq, val));
                        }
                        return None;
                    }
                    Token::Contains => {
                        self.advance();
                        self.advance();
                        if let Token::Word(v) = &self.cur {
                            let val = v.clone();
                            self.advance();
                            return Some(Node::Compare(name_clone, CompareOp::Contains, val));
                        }
                        return None;
                    }
                    _ => {
                        let s = name_clone.clone();
                        if s.len() >= 2 && s.starts_with('/') && s.ends_with('/') {
                            let pat = s[1..s.len()-1].to_string();
                            self.advance();
                            return Some(Node::Regex(pat));
                        }
                        self.advance();
                        return Some(Node::Word(s));
                    }
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_expression() {
        let mut p = Parser::new("name:foo AND (bar OR baz)");
        let ast = p.parse();
        assert!(ast.is_some());
        match ast.unwrap() {
            Node::And(left, right) => {
                match *left {
                    Node::Field(ref n, ref v) => {
                        assert_eq!(n, "name");
                        assert_eq!(v, "foo");
                    }
                    _ => panic!("left not field"),
                }
                match *right {
                    Node::Group(boxed) => {
                        match *boxed {
                            Node::Or(a, b) => {
                                match *a {
                                    Node::Word(ref w) => assert_eq!(w, "bar"),
                                    _ => panic!("expected bar"),
                                }
                                match *b {
                                    Node::Word(ref w) => assert_eq!(w, "baz"),
                                    _ => panic!("expected baz"),
                                }
                            }
                            _ => panic!("expected OR inside group"),
                        }
                    }
                    _ => panic!("right not group"),
                }
            }
            _ => panic!("expected AND at top"),
        }
    }

    #[test]
    fn parse_compare_without_colon() {
        let mut p = Parser::new("size<1000");
        let ast = p.parse();
        assert!(ast.is_some());
        match ast.unwrap() {
            Node::Compare(ref field, ref op, ref val) => {
                assert_eq!(field, "size");
                assert_eq!(val, "1000");
                assert_eq!(*op, CompareOp::Smaller);
            }
            _ => panic!("expected Compare node"),
        }
    }

    #[test]
    fn parse_regex_literal() {
        let mut p = Parser::new("/ab[0-9]+/");
        let ast = p.parse();
        assert!(ast.is_some());
        match ast.unwrap() {
            Node::Regex(pat) => assert_eq!(pat, "ab[0-9]+"),
            _ => panic!("expected regex node"),
        }
    }

    #[test]
    fn lexer_tokens_for_compare() {
        let mut lx = crate::query::lexer::Lexer::new("size<1000");
        let (t1, s1) = lx.next_token();
        assert_eq!(t1, crate::query::lexer::Token::Word(s1.clone().unwrap()));
        let (t2, _) = lx.next_token();
        assert_eq!(t2, crate::query::lexer::Token::Smaller);
        let (t3, s3) = lx.next_token();
        assert_eq!(t3, crate::query::lexer::Token::Word(s3.clone().unwrap()));
    }
}
