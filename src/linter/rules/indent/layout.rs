//! Shared source-token offset graph for the indentation visitors.
use crate::linter::{Fix, LintContext};
use oxc::span::Span;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(super) struct Options {
    pub indent_char: char,
    pub indent_size: usize,
    pub indent_script: bool,
    pub switch_case: i32,
    pub align_attributes_vertically: bool,
    pub ignored_nodes: Vec<String>,
}
impl Options {
    pub fn parse(value: Option<&serde_json::Value>) -> Self {
        let value = value.and_then(|v| v.as_array()).and_then(|a| a.first());
        let tab = value.and_then(|v| v.get("indent")).and_then(|v| v.as_str()) == Some("tab");
        Self {
            indent_char: if tab { '\t' } else { ' ' },
            indent_size: if tab {
                1
            } else {
                value
                    .and_then(|v| v.get("indent"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2) as usize
            },
            indent_script: value
                .and_then(|v| v.get("indentScript"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            switch_case: value
                .and_then(|v| v.get("switchCase"))
                .and_then(|v| v.as_i64())
                .unwrap_or(1) as i32,
            align_attributes_vertically: !tab
                && value
                    .and_then(|v| v.get("alignAttributesVertically"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            ignored_nodes: value
                .and_then(|v| v.get("ignoredNodes"))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum RegionKind {
    Expression,
    Const,
    Binding,
    Parameters,
}
#[derive(Clone, Copy)]
pub(super) struct Region {
    pub span: Span,
    pub kind: RegionKind,
}
#[derive(Clone, Copy)]
pub(super) struct Token {
    pub span: Span,
    pub comment: bool,
}
#[derive(Clone, Copy)]
enum Offset {
    Start(i32),
    Relative(u32, i32),
    Align(u32, usize),
}
pub(super) struct Layout<'s> {
    pub source: &'s str,
    pub tokens: Vec<Token>,
    pub options: Options,
    pub regions: Vec<Region>,
    offsets: HashMap<u32, Offset>,
    cache: HashMap<u32, usize>,
    ignores: Vec<Span>,
    lines: Vec<u32>,
}
impl<'s> Layout<'s> {
    pub fn new(source: &'s str, options: Options) -> Self {
        let mut lines = vec![0];
        let mut iter = source.char_indices().peekable();
        while let Some((i, ch)) = iter.next() {
            if ch == '\r' {
                if iter.peek().is_some_and(|(_, c)| *c == '\n') {
                    iter.next();
                    lines.push(i as u32 + 2);
                } else {
                    lines.push(i as u32 + 1);
                }
            } else if matches!(ch, '\n' | '\u{2028}' | '\u{2029}') {
                lines.push((i + ch.len_utf8()) as u32);
            }
        }
        Self {
            source,
            tokens: vec![],
            options,
            regions: vec![],
            offsets: HashMap::new(),
            cache: HashMap::new(),
            ignores: vec![],
            lines,
        }
    }
    pub fn add_token(&mut self, span: Span, comment: bool) {
        if span.start < span.end && span.end as usize <= self.source.len() {
            self.tokens.push(Token { span, comment });
        }
    }
    pub fn sort(&mut self) {
        self.tokens.sort_by_key(|t| (t.span.start, t.span.end));
        self.tokens.dedup_by_key(|t| (t.span.start, t.span.end));
    }
    pub fn text(&self, i: usize) -> &'s str {
        let s = self.tokens[i].span;
        &self.source[s.start as usize..s.end as usize]
    }
    pub fn first(&self, s: Span) -> Option<usize> {
        let start = self.tokens.partition_point(|t| t.span.start < s.start);
        (start..self.tokens.len())
            .take_while(|&i| self.tokens[i].span.start < s.end)
            .find(|&i| !self.tokens[i].comment && self.tokens[i].span.end <= s.end)
    }
    pub fn last(&self, s: Span) -> Option<usize> {
        let end = self.tokens.partition_point(|t| t.span.end <= s.end);
        (0..end)
            .rev()
            .take_while(|&i| self.tokens[i].span.start >= s.start)
            .find(|&i| !self.tokens[i].comment)
    }
    pub fn before(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&n| !self.tokens[n].comment)
    }
    pub fn after(&self, i: usize) -> Option<usize> {
        (i + 1..self.tokens.len()).find(|&n| !self.tokens[n].comment)
    }
    pub fn first_last(&self, s: Span, border: u32) -> Option<(usize, usize)> {
        let (mut first, mut last) = (self.first(s)?, self.last(s)?);
        while let (Some(left), Some(right)) = (self.before(first), self.after(last)) {
            if self.tokens[left].span.start < border
                || self.text(left) != "("
                || self.text(right) != ")"
            {
                break;
            }
            first = left;
            last = right;
        }
        Some((first, last))
    }
    pub fn set(&mut self, i: usize, offset: i32, base: usize) {
        if i != base {
            self.offsets.insert(
                self.tokens[i].span.start,
                Offset::Relative(self.tokens[base].span.start, offset),
            );
        }
    }
    pub fn start(&mut self, i: usize, offset: i32) {
        self.offsets
            .insert(self.tokens[i].span.start, Offset::Start(offset));
    }
    pub fn align(&mut self, i: usize, columns: usize, base: usize) {
        if i != base {
            self.offsets.insert(
                self.tokens[i].span.start,
                Offset::Align(self.tokens[base].span.start, columns),
            );
        }
    }
    pub fn copy(&mut self, i: usize, other: usize) {
        if let Some(o) = self.offsets.get(&self.tokens[other].span.start).copied() {
            self.offsets.insert(self.tokens[i].span.start, o);
        }
    }
    pub fn ignore(&mut self, s: Span) {
        self.ignores.push(s);
    }
    pub fn line_of(&self, offset: u32) -> usize {
        self.lines
            .partition_point(|&v| v <= offset)
            .saturating_sub(1)
    }
    pub fn column(&self, offset: u32) -> usize {
        self.source[self.lines[self.line_of(offset)] as usize..offset as usize]
            .encode_utf16()
            .count()
    }
    pub fn is_beginning(&self, s: Span) -> bool {
        self.first(s).is_some_and(|i| {
            self.before(i).is_none_or(|prev| {
                self.line_of(self.tokens[prev].span.end) < self.line_of(self.tokens[i].span.start)
            })
        })
    }
    pub fn list(
        &mut self,
        nodes: &[Span],
        base: Span,
        end: Option<Span>,
        offset: i32,
        align: bool,
    ) {
        let (Some(anchor), Some(mut previous)) = (self.first(base), self.last(base)) else {
            return;
        };
        let columns = if align {
            nodes.first().filter(|s| !self.is_beginning(**s)).map(|s| {
                let start = self.lines[self.line_of(s.start)];
                let indent = self.source[start as usize..s.start as usize]
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .map(char::len_utf16)
                    .sum::<usize>();
                self.column(s.start).saturating_sub(indent)
            })
        } else {
            None
        };
        let assign = |this: &mut Self, i| {
            if let Some(c) = columns {
                this.align(i, c, anchor)
            } else {
                this.set(i, offset, anchor)
            }
        };
        for node in nodes {
            if let Some((first, last)) = self.first_last(*node, self.tokens[previous].span.end) {
                for i in previous + 1..first {
                    assign(self, i);
                }
                assign(self, first);
                previous = last;
            }
        }
        if let Some(end) = end.and_then(|s| self.first(s)) {
            for i in previous + 1..end {
                assign(self, i);
            }
            self.set(end, 0, anchor);
        }
    }
    fn expected(&self, key: u32, seen: &mut HashSet<u32>) -> Option<usize> {
        if let Some(v) = self.cache.get(&key) {
            return Some(*v);
        }
        if !seen.insert(key) {
            return None;
        }
        let result = match *self.offsets.get(&key)? {
            Offset::Start(n) => Some((n * self.options.indent_size as i32).max(0) as usize),
            Offset::Relative(base, n) => self
                .expected(base, seen)
                .map(|v| (v as i64 + n as i64 * self.options.indent_size as i64).max(0) as usize),
            Offset::Align(base, n) => self.expected(base, seen).map(|v| v + n),
        };
        seen.remove(&key);
        result
    }
    fn get(&self, i: usize) -> Option<usize> {
        self.expected(self.tokens[i].span.start, &mut HashSet::new())
    }
    fn validate(&self, i: usize, expected: usize, ctx: &mut LintContext<'_>) {
        let token = self.tokens[i];
        let start = self.lines[self.line_of(token.span.start)];
        let prefix = &self.source[start as usize..token.span.start as usize];
        if !prefix.trim().is_empty() {
            return;
        }
        let actual = prefix.encode_utf16().count();
        let wrong = prefix.chars().any(|c| c != self.options.indent_char);
        if actual != expected {
            let unit = if self.options.indent_char == '\t' {
                "tab"
            } else {
                "space"
            };
            let actual_unit = if wrong { "whitespace" } else { unit };
            let plural = |n| if n == 1 { "" } else { "s" };
            let span = Span::new(start, token.span.start);
            ctx.diagnostic_with_fix(format!("Expected indentation of {expected} {unit}{} but found {actual} {actual_unit}{}.",plural(expected),plural(actual)),span,Fix{span,replacement:self.options.indent_char.to_string().repeat(expected)});
        } else {
            for (offset, ch) in prefix
                .char_indices()
                .filter(|(_, c)| *c != self.options.indent_char)
            {
                let span = Span::new(
                    start + offset as u32,
                    start + (offset + ch.len_utf8()) as u32,
                );
                ctx.diagnostic_with_fix(
                    format!(
                        "Expected {} character, but found {} character.",
                        serde_json::to_string(&self.options.indent_char.to_string()).unwrap(),
                        serde_json::to_string(&ch.to_string()).unwrap()
                    ),
                    span,
                    Fix {
                        span,
                        replacement: self.options.indent_char.to_string(),
                    },
                );
            }
        }
    }
    pub fn report(&mut self, ctx: &mut LintContext<'_>) {
        let mut previous: Option<usize> = None;
        let mut comments = Vec::new();
        let mut cursor = 0;
        while cursor < self.tokens.len() {
            let line = self.line_of(self.tokens[cursor].span.start);
            let start = cursor;
            while cursor < self.tokens.len() && self.line_of(self.tokens[cursor].span.start) == line
            {
                cursor += 1;
            }
            let indexes = (start..cursor).collect::<Vec<_>>();
            if indexes.iter().all(|&i| self.tokens[i].comment) {
                comments.push(start);
                continue;
            }
            let mut expected = None;
            for &i in &indexes {
                if self.ignores.iter().any(|s| {
                    s.start <= self.tokens[i].span.start && self.tokens[i].span.start < s.end
                }) {
                    break;
                }
                if let Some(v) = self.get(i) {
                    expected = Some(v);
                    break;
                }
            }
            let cache_value = if expected.is_some() {
                indexes.iter().filter_map(|&i| self.get(i)).min().unwrap()
            } else {
                self.column(self.tokens[start].span.start)
            };
            for &i in &indexes {
                let key = self.tokens[i].span.start;
                if self.offsets.contains_key(&key) {
                    self.cache.entry(key).or_insert(cache_value);
                }
            }
            if let Some(expected) = expected {
                if let Some(&first) = comments.first() {
                    if previous.is_some_and(|p| {
                        self.line_of(self.tokens[p].span.end)
                            < self.line_of(self.tokens[first].span.start)
                    }) {
                        self.validate(first, expected, ctx);
                    }
                    previous = comments.last().copied();
                }
                if previous.is_some_and(|p| self.line_of(self.tokens[p].span.end) < line) {
                    self.validate(start, expected, ctx);
                }
            }
            previous = Some(cursor - 1);
            comments.clear();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offset_graph_and_utf16_columns() {
        let mut l = Layout::new("😀 a\r\n\t b", Options::parse(None));
        l.add_token(Span::new(5, 6), false);
        l.add_token(Span::new(10, 11), false);
        l.start(0, 0);
        l.set(1, 1, 0);
        assert_eq!(l.get(1), Some(2));
        assert_eq!(l.column(5), 3);
        assert_eq!(l.line_of(10), 1);
        l.set(0, 1, 1);
        assert_eq!(l.get(1), None);
    }
}
