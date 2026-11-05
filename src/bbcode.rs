//! Minimal, forgiving parser for the BBCode dialect Steam uses in announcements.
//!
//! It produces a flat list of blocks that the UI renders natively. Unknown tags
//! are dropped (their text is kept), media embeds are skipped.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading(u8, Vec<Span>),
    Paragraph(Vec<Span>),
    /// Bullet at the given nesting depth (1 = top level).
    Bullet(u8, Vec<Span>),
    Rule,
}

#[derive(Clone, Copy, Default)]
struct Style {
    bold: u8,
    italic: u8,
    strike: u8,
}

struct Builder {
    blocks: Vec<Block>,
    spans: Vec<Span>,
    style: Style,
    links: Vec<String>,
    heading: Option<u8>,
    list_depth: u8,
    in_item: bool,
    skip_depth: u32,
}

impl Builder {
    fn push_text(&mut self, text: &str) {
        if self.skip_depth > 0 || text.is_empty() {
            return;
        }
        let link = self.links.last().cloned();
        let (bold, italic, strike) =
            (self.style.bold > 0 || self.heading.is_some(), self.style.italic > 0, self.style.strike > 0);
        if let Some(last) = self.spans.last_mut()
            && last.bold == bold
            && last.italic == italic
            && last.strike == strike
            && last.link == link
        {
            last.text.push_str(text);
            return;
        }
        self.spans.push(Span { text: text.to_owned(), bold, italic, strike, link });
    }

    fn flush(&mut self) {
        // Collapse whitespace runs and trim the block.
        let mut spans: Vec<Span> = std::mem::take(&mut self.spans);
        for s in &mut spans {
            let mut out = String::with_capacity(s.text.len());
            let mut prev_space = false;
            for c in s.text.chars() {
                if c.is_whitespace() {
                    if !prev_space {
                        out.push(' ');
                    }
                    prev_space = true;
                } else {
                    out.push(c);
                    prev_space = false;
                }
            }
            s.text = out;
        }
        if let Some(first) = spans.first_mut() {
            first.text = first.text.trim_start().to_owned();
        }
        if let Some(last) = spans.last_mut() {
            last.text = last.text.trim_end().to_owned();
        }
        spans.retain(|s| !s.text.is_empty());
        if spans.is_empty() {
            return;
        }
        let block = if let Some(level) = self.heading {
            Block::Heading(level, spans)
        } else if self.in_item {
            Block::Bullet(self.list_depth.max(1), spans)
        } else {
            Block::Paragraph(spans)
        };
        self.blocks.push(block);
    }

    fn open(&mut self, name: &str, arg: Option<&str>) {
        match name {
            "b" | "strong" => self.style.bold += 1,
            "i" | "em" => self.style.italic += 1,
            "strike" | "s" => self.style.strike += 1,
            "u" => {}
            "h1" | "h2" | "h3" | "h4" | "h5" => {
                self.flush();
                self.heading = Some(name[1..].parse().unwrap_or(3));
            }
            "p" | "tr" | "quote" | "code" => self.flush(),
            "list" | "olist" => {
                self.flush();
                self.list_depth = self.list_depth.saturating_add(1);
            }
            "*" | "li" => {
                self.flush();
                self.in_item = self.list_depth > 0;
            }
            "hr" => {
                self.flush();
                self.blocks.push(Block::Rule);
            }
            "td" | "th" => {
                if !self.spans.is_empty() {
                    self.push_text("  ·  ");
                }
            }
            "url" => self.links.push(arg.map(str::to_owned).unwrap_or_default()),
            "previewyoutube" => {
                self.flush();
                if let Some(id) = arg.and_then(|a| a.split(';').next()) {
                    self.links.push(format!("https://www.youtube.com/watch?v={id}"));
                    self.push_text("▶ Watch video on YouTube");
                    self.links.pop();
                    self.flush();
                }
                self.skip_depth += 1;
            }
            "img" | "video" | "dynamiclink" | "spoiler" => self.skip_depth += 1,
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "b" | "strong" => self.style.bold = self.style.bold.saturating_sub(1),
            "i" | "em" => self.style.italic = self.style.italic.saturating_sub(1),
            "strike" | "s" => self.style.strike = self.style.strike.saturating_sub(1),
            "h1" | "h2" | "h3" | "h4" | "h5" => {
                self.flush();
                self.heading = None;
            }
            "p" | "tr" | "quote" | "code" | "table" => self.flush(),
            "list" | "olist" => {
                self.flush();
                self.list_depth = self.list_depth.saturating_sub(1);
                self.in_item = self.list_depth > 0;
            }
            "*" | "li" => {
                self.flush();
                // Text after a nested [/*] still belongs to the parent item.
                self.in_item = self.list_depth > 0;
            }
            "url" => {
                if let Some(link) = self.links.pop() {
                    // [url]https://…[/url] has no argument: the text is the target.
                    if link.is_empty()
                        && let Some(last) = self.spans.last_mut()
                        && last.link.as_deref() == Some("")
                    {
                        last.link = Some(last.text.trim().to_owned());
                    }
                }
            }
            "previewyoutube" | "img" | "video" | "dynamiclink" | "spoiler" => {
                self.skip_depth = self.skip_depth.saturating_sub(1)
            }
            _ => {}
        }
    }
}

/// Parse a Steam BBCode body into blocks.
pub fn parse(src: &str) -> Vec<Block> {
    let mut b = Builder {
        blocks: Vec::new(),
        spans: Vec::new(),
        style: Style::default(),
        links: Vec::new(),
        heading: None,
        list_depth: 0,
        in_item: false,
        skip_depth: 0,
    };
    let src = src.replace("\r\n", "\n");
    let mut text = String::new();
    let mut rest = src.as_str();

    while let Some(c) = rest.chars().next() {
        if c == '\\' && rest[1..].starts_with('[') {
            text.push('[');
            rest = &rest[2..];
            continue;
        }
        if c == '\n' {
            b.push_text(&std::mem::take(&mut text));
            // A blank line or a newline outside of inline markup starts a new block.
            b.flush();
            rest = &rest[1..];
            continue;
        }
        if c == '['
            && let Some(end) = rest.find(']')
        {
            let inner = &rest[1..end];
            if let Some((closing, name, arg)) = parse_tag(inner) {
                b.push_text(&std::mem::take(&mut text));
                if closing {
                    b.close(&name);
                } else {
                    b.open(&name, arg.as_deref());
                }
                rest = &rest[end + 1..];
                continue;
            }
        }
        text.push(c);
        rest = &rest[c.len_utf8()..];
    }
    b.push_text(&text);
    b.flush();
    b.blocks.retain(|blk| !is_placeholder(blk));
    b.blocks
}

fn parse_tag(inner: &str) -> Option<(bool, String, Option<String>)> {
    let (closing, body) = match inner.strip_prefix('/') {
        Some(b) => (true, b),
        None => (false, inner),
    };
    let (name, arg) = match body.split_once('=') {
        Some((n, a)) => (n, Some(a.trim_matches(['"', '\'']).to_owned())),
        None => match body.split_once(' ') {
            Some((n, _attrs)) => (n, None),
            None => (body, None),
        },
    };
    let name = name.trim().to_ascii_lowercase();
    let valid = !name.is_empty() && name.len() <= 16 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '*');
    valid.then_some((closing, name, arg))
}

fn is_placeholder(block: &Block) -> bool {
    let spans = match block {
        Block::Paragraph(s) | Block::Heading(_, s) | Block::Bullet(_, s) => s,
        Block::Rule => return false,
    };
    let text: String = spans.iter().map(|s| s.text.as_str()).collect();
    let t = text.trim();
    t.starts_with("{STEAM_CLAN_IMAGE}") || t.starts_with("{STEAM_CLAN_LOC_IMAGE}")
}

/// Plain-text preview of a body, at most `max_chars` characters.
pub fn excerpt(src: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for block in parse(src) {
        let spans = match &block {
            Block::Paragraph(s) | Block::Heading(_, s) | Block::Bullet(_, s) => s,
            Block::Rule => continue,
        };
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        let t = text.trim();
        // Skip Valve-style section markers such as "[ GAMEPLAY ]".
        if t.is_empty() || (t.starts_with('[') && t.ends_with(']')) {
            continue;
        }
        if !out.is_empty() {
            out.push_str(" · ");
        }
        out.push_str(t);
        if out.chars().count() >= max_chars {
            break;
        }
    }
    if out.chars().count() > max_chars {
        let cut: String = out.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", cut.trim_end())
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn steam_patch_notes() {
        let src = "[p]\\[ GAMEPLAY ][/p][list][*][p]Bomb defusal is [b]faster[/b].[/p][/*][*][p]Fixed a bug.[/p][/*][/list][p]\\[ MISC ][/p]";
        let blocks = parse(src);
        assert_eq!(blocks.len(), 4, "{blocks:#?}");
        assert!(matches!(&blocks[0], Block::Paragraph(s) if text(s) == "[ GAMEPLAY ]"));
        match &blocks[1] {
            Block::Bullet(1, s) => {
                assert_eq!(text(s), "Bomb defusal is faster.");
                assert!(s.iter().any(|x| x.bold && x.text == "faster"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(&blocks[3], Block::Paragraph(s) if text(s) == "[ MISC ]"));
    }

    #[test]
    fn headings_links_and_media() {
        let src = "[h2]Patch 2.31[/h2]\n[img]{STEAM_CLAN_IMAGE}/x.png[/img]\nRead [url=https://x.y]more[/url] or [url]https://a.b[/url]";
        let blocks = parse(src);
        assert!(matches!(&blocks[0], Block::Heading(2, s) if text(s) == "Patch 2.31"));
        match &blocks[1] {
            Block::Paragraph(s) => {
                assert_eq!(text(s), "Read more or https://a.b");
                assert_eq!(s[1].link.as_deref(), Some("https://x.y"));
                assert_eq!(s.last().unwrap().link.as_deref(), Some("https://a.b"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn nested_lists() {
        let blocks = parse("[list][*]A[list][*]B[/*][/list][/*][/list]");
        assert_eq!(
            blocks
                .iter()
                .map(|b| match b {
                    Block::Bullet(d, _) => *d,
                    _ => 0,
                })
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn excerpt_is_bounded() {
        let e = excerpt("[p]Hello world[/p][p]Second paragraph here[/p]", 15);
        assert!(e.chars().count() <= 15);
        assert!(e.ends_with('…'));
        assert_eq!(excerpt("[p]Hi[/p]", 50), "Hi");
        assert_eq!(excerpt(r"[p]\[ GAMEPLAY ][/p][list][*]Faster[/*][/list]", 50), "Faster");
    }

    #[test]
    fn unbalanced_input_does_not_panic() {
        for s in ["[", "]", "[b]", "[/list][/list]", "[url=", "\\", "[*]x", "ü[ä]"] {
            let _ = parse(s);
        }
    }
}
