//! Minimal JSON reader/writer (no dependencies).

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

pub static NULL: Value = Value::Null;

impl Value {
    pub fn get(&self, key: &str) -> &Value {
        if let Value::Obj(kv) = self {
            for (k, v) in kv {
                if k == key {
                    return v;
                }
            }
        }
        &NULL
    }
    pub fn idx(&self, i: usize) -> &Value {
        if let Value::Arr(a) = self {
            if i < a.len() {
                return &a[i];
            }
        }
        &NULL
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    pub fn f(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }
    pub fn f_or(&self, d: f64) -> f64 {
        match self {
            Value::Num(n) => *n,
            _ => d,
        }
    }
    pub fn b(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            _ => false,
        }
    }
    pub fn s(&self) -> &str {
        match self {
            Value::Str(s) => s.as_str(),
            _ => "",
        }
    }
    pub fn arr(&self) -> &[Value] {
        match self {
            Value::Arr(a) => a.as_slice(),
            _ => &[],
        }
    }
    pub fn obj(&self) -> &[(String, Value)] {
        match self {
            Value::Obj(o) => o.as_slice(),
            _ => &[],
        }
    }
    pub fn v3(&self) -> [f64; 3] {
        [self.idx(0).f(), self.idx(1).f(), self.idx(2).f()]
    }
    pub fn v2(&self) -> [f64; 2] {
        [self.idx(0).f(), self.idx(1).f()]
    }
}

pub fn parse(s: &str) -> Result<Value, String> {
    let mut p = Parser { b: s.as_bytes(), i: 0 };
    p.ws();
    let v = p.value()?;
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() {
            match self.b[self.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }
    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        if self.i >= self.b.len() {
            return Err("eof".into());
        }
        match self.b[self.i] {
            b'{' => {
                self.i += 1;
                let mut kv = Vec::new();
                self.ws();
                if self.i < self.b.len() && self.b[self.i] == b'}' {
                    self.i += 1;
                    return Ok(Value::Obj(kv));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if self.i >= self.b.len() || self.b[self.i] != b':' {
                        return Err("expected :".into());
                    }
                    self.i += 1;
                    let v = self.value()?;
                    kv.push((k, v));
                    self.ws();
                    if self.i >= self.b.len() {
                        return Err("eof in object".into());
                    }
                    match self.b[self.i] {
                        b',' => self.i += 1,
                        b'}' => {
                            self.i += 1;
                            return Ok(Value::Obj(kv));
                        }
                        _ => return Err("expected , or }".into()),
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.i < self.b.len() && self.b[self.i] == b']' {
                    self.i += 1;
                    return Ok(Value::Arr(a));
                }
                loop {
                    let v = self.value()?;
                    a.push(v);
                    self.ws();
                    if self.i >= self.b.len() {
                        return Err("eof in array".into());
                    }
                    match self.b[self.i] {
                        b',' => self.i += 1,
                        b']' => {
                            self.i += 1;
                            return Ok(Value::Arr(a));
                        }
                        _ => return Err("expected , or ]".into()),
                    }
                }
            }
            b'"' => Ok(Value::Str(self.string()?)),
            b't' => {
                self.lit("true")?;
                Ok(Value::Bool(true))
            }
            b'f' => {
                self.lit("false")?;
                Ok(Value::Bool(false))
            }
            b'n' => {
                self.lit("null")?;
                Ok(Value::Null)
            }
            _ => self.number(),
        }
    }
    fn lit(&mut self, w: &str) -> Result<(), String> {
        let wb = w.as_bytes();
        if self.b.len() >= self.i + wb.len() && &self.b[self.i..self.i + wb.len()] == wb {
            self.i += wb.len();
            Ok(())
        } else {
            Err(format!("bad literal at {}", self.i))
        }
    }
    fn number(&mut self) -> Result<Value, String> {
        let st = self.i;
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => self.i += 1,
                _ => break,
            }
        }
        if st == self.i {
            return Err(format!("unexpected char at {}", self.i));
        }
        let s = std::str::from_utf8(&self.b[st..self.i]).map_err(|_| "utf8")?;
        s.parse::<f64>().map(Value::Num).map_err(|_| format!("bad number {}", s))
    }
    fn string(&mut self) -> Result<String, String> {
        if self.i >= self.b.len() || self.b[self.i] != b'"' {
            return Err("expected string".into());
        }
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        while self.i < self.b.len() {
            let c = self.b[self.i];
            match c {
                b'"' => {
                    self.i += 1;
                    return String::from_utf8(out).map_err(|_| "utf8".into());
                }
                b'\\' => {
                    self.i += 1;
                    if self.i >= self.b.len() {
                        break;
                    }
                    let e = self.b[self.i];
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            if self.i + 4 > self.b.len() {
                                break;
                            }
                            let h = std::str::from_utf8(&self.b[self.i..self.i + 4]).unwrap_or("0");
                            let mut cp = u32::from_str_radix(h, 16).unwrap_or(0x3f);
                            self.i += 4;
                            if (0xD800..0xDC00).contains(&cp)
                                && self.i + 6 <= self.b.len()
                                && self.b[self.i] == b'\\'
                                && self.b[self.i + 1] == b'u'
                            {
                                let h2 = std::str::from_utf8(&self.b[self.i + 2..self.i + 6]).unwrap_or("0");
                                let lo = u32::from_str_radix(h2, 16).unwrap_or(0);
                                if (0xDC00..0xE000).contains(&lo) {
                                    cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                    self.i += 6;
                                }
                            }
                            let ch = char::from_u32(cp).unwrap_or('?');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => out.push(e),
                    }
                }
                _ => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
        Err("unterminated string".into())
    }
}

/// Format a float compactly for JSON output.
pub fn fnum(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    if v == v.trunc() && v.abs() < 1e12 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if s == "-0" { "0".to_string() } else { s }
    }
}

/// Escape a string for JSON output (with quotes).
pub fn fstr(t: &str) -> String {
    let mut s = String::with_capacity(t.len() + 2);
    s.push('"');
    for c in t.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            c if (c as u32) < 0x20 => s.push_str(&format!("\\u{:04x}", c as u32)),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}
