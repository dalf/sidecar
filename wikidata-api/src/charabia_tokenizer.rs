//! A library that bridges between tantivy and charabia.
#![forbid(unsafe_code)]

extern crate charabia;
extern crate tantivy;

use self::charabia::{Tokenize, Segment};
use self::tantivy::tokenizer::{BoxTokenStream, Token, TokenStream, Tokenizer};

#[derive(Clone)]
pub struct CharabiaTokenizer;

#[derive(Clone)]
pub struct CharabiaSegmentTokenizer;

pub struct CharabiaTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl TokenStream for CharabiaTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index = self.index + 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}

impl Tokenizer for CharabiaTokenizer {
    fn token_stream<'a>(&self, text: &'a str) -> BoxTokenStream<'a> {
        let mut tokens = Vec::new();
        let mut position = 0;
        for i in text.tokenize() {
            tokens.push(Token {
                offset_from: i.byte_start,
                offset_to: i.byte_end,
                position: position,
                text: String::from(i.lemma.to_string()),
                position_length: i.byte_end - i.byte_start,
            });
            position += 1;
        }
        BoxTokenStream::from(CharabiaTokenStream { tokens, index: 0 })
    }
}

impl Tokenizer for CharabiaSegmentTokenizer {
    fn token_stream<'a>(&self, text: &'a str) -> BoxTokenStream<'a> {
        let mut tokens = Vec::new();
        let mut position = 0;
        for i in text.segment_str() {
            tokens.push(Token {
                offset_from: position,
                offset_to: position + i.len(),
                position: position,
                text: String::from(i),
                position_length: i.len(),
            });
            position += i.len();
        }
        BoxTokenStream::from(CharabiaTokenStream { tokens, index: 0 })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works_fr_segment() {
        use super::tantivy::tokenizer::Tokenizer;

        let tokenizer = crate::charabia_tokenizer::CharabiaSegmentTokenizer {};
        let mut token_stream = tokenizer.token_stream(
            "Le Petit Prince est une œuvre de langue française, la plus connue d'Antoine de Saint-Exupéry.",
        );
        let mut tokens = Vec::new();
        let mut token_text = Vec::new();
        while let Some(token) = token_stream.next() {
            tokens.push(token.clone());
            token_text.push(token.text.clone());
            println!("{} ({},{}) {} {}", token.position, token.offset_from, token.offset_to, token.text, token.position_length);
        }
    }

    #[test]
    fn it_works_fr() {
        use super::tantivy::tokenizer::Tokenizer;

        let tokenizer = crate::charabia_tokenizer::CharabiaTokenizer {};
        let mut token_stream = tokenizer.token_stream(
            "Le Petit Prince est une œuvre de langue française, la plus connue d'Antoine de Saint-Exupéry.",
        );
        let mut tokens = Vec::new();
        let mut token_text = Vec::new();
        while let Some(token) = token_stream.next() {
            tokens.push(token.clone());
            token_text.push(token.text.clone());
            println!("{} ({},{}) {} {}", token.position, token.offset_from, token.offset_to, token.text, token.position_length);
        }
        // check tokenized text
        assert_eq!(
            token_text,
            vec![
                "le",
                " ",
                "petit",
                " ",
                "prince",
                " ",
                "est",
                " ",
                "une",
                " ",
                "oeuvre",
                " ",
                "de",
                " ",
                "langue",
                " ",
                "francaise",
                ",",
                " ",
                "la",
                " ",
                "plus",
                " ",
                "connue",
                " ",
                "d'",
                "antoine",
                " ",
                "de",
                " ",
                "saint",
                "-",
                "exupery",
                ".",
            ]
        );
    }

    #[test]
    fn it_works_zh() {
        use super::tantivy::tokenizer::Tokenizer;

        let tokenizer = crate::charabia_tokenizer::CharabiaTokenizer {};
        let mut token_stream = tokenizer.token_stream(
            "张华考上了北京大学；李萍进了中等技术学校；我在百货公司当售货员：我们都有光明的前途",
        );
        let mut tokens = Vec::new();
        let mut token_text = Vec::new();
        while let Some(token) = token_stream.next() {
            tokens.push(token.clone());
            token_text.push(token.text.clone());
        }
        // offset should be byte-indexed
        assert_eq!(tokens[0].offset_from, 0);
        assert_eq!(tokens[0].offset_to, "张华".bytes().len());
        assert_eq!(tokens[1].offset_from, "张华".bytes().len());
        // check tokenized text
        assert_eq!(
            token_text,
            vec![
                "张华",
                "考上",
                "了",
                "北京大学",
                "；",
                "李萍",
                "进",
                "了",
                "中等",
                "技术学校",
                "；",
                "我",
                "在",
                "百货公司",
                "当",
                "售货员",
                "：",
                "我们",
                "都",
                "有",
                "光明",
                "的",
                "前途"
            ]
        );
    }
}
