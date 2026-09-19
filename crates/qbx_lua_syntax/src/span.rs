#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub const fn empty(at: u32) -> Self {
        Self { start: at, end: at }
    }

    pub fn to(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.end <= self.start
    }

    pub fn contains(self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }

    pub fn contains_inclusive(self, offset: u32) -> bool {
        self.start <= offset && offset <= self.end
    }

    pub fn contains_span(self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    pub fn text(self, source: &str) -> &str {
        source.get(self.start as usize..self.end as usize).unwrap_or("")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, PartialOrd, Ord)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Maps byte offsets to zero-based line/column pairs.
#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<u32>,
    len: u32,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        Self { line_starts, len: source.len() as u32 }
    }

    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    pub fn line_start(&self, line: u32) -> u32 {
        self.line_starts.get(line as usize).copied().unwrap_or(self.len)
    }

    pub fn line_of(&self, offset: u32) -> u32 {
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line as u32,
            Err(next) => next as u32 - 1,
        }
    }

    pub fn line_span(&self, line: u32) -> Span {
        Span::new(self.line_start(line), self.line_start(line + 1))
    }

    /// Column counted in Unicode scalar values.
    pub fn line_col(&self, source: &str, offset: u32) -> LineCol {
        let offset = offset.min(self.len);
        let line = self.line_of(offset);
        let start = self.line_start(line) as usize;
        let col = source
            .get(start..offset as usize)
            .map(|s| s.chars().count() as u32)
            .unwrap_or(offset - start as u32);
        LineCol { line, col }
    }

    /// Column counted in UTF-16 code units, as the Language Server Protocol expects by default.
    pub fn line_col_utf16(&self, source: &str, offset: u32) -> LineCol {
        let offset = floor_char_boundary(source, offset.min(self.len) as usize);
        let line = self.line_of(offset as u32);
        let start = self.line_start(line) as usize;
        let col = source[start..offset].chars().map(|c| c.len_utf16() as u32).sum();
        LineCol { line, col }
    }

    pub fn offset_utf16(&self, source: &str, pos: LineCol) -> u32 {
        if pos.line >= self.line_count() {
            return self.len;
        }
        let start = self.line_start(pos.line) as usize;
        let end = self.line_start(pos.line + 1) as usize;
        let mut units = 0u32;
        for (i, c) in source[start..end].char_indices() {
            if units >= pos.col || c == '\n' || c == '\r' {
                return (start + i) as u32;
            }
            units += c.len_utf16() as u32;
        }
        end as u32
    }
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
