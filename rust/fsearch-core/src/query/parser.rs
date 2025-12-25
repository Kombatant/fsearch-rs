use crate::query::lexer::{Lexer, Token};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    Not(Box<Node>),
    Word(String),
    Field(String, String),
    Group(Box<Node>),
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
                // expect a word next
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
                // name: with empty value
                let field_name = name.clone();
                self.advance();
                Some(Node::Field(field_name, String::new()))
            }
            Token::Word(w) => {
                let s = w.clone();
                self.advance();
                Some(Node::Word(s))
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
        // expect top-level AND
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
            _ => panic!("expected AND at top")
        }
    }
}
