//! D-Bus tối giản (chỉ những gì HFastKey cần), không thư viện ngoài, một luồng.
//!
//! Hỗ trợ: kết nối session bus qua socket Unix, xác thực EXTERNAL, mã hoá/giải
//! mã thông điệp little-endian với các kiểu y b n q i u x t d s o g a ( ) { } v.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

pub const METHOD_CALL: u8 = 1;
pub const METHOD_RETURN: u8 = 2;
pub const ERROR: u8 = 3;
pub const SIGNAL: u8 = 4;
pub const NO_REPLY_EXPECTED: u8 = 1;

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    Byte(u8),
    Bool(bool),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Path(String),
    Sig(String),
    /// Mảng byte (ay), giữ gọn thay vì từng phần tử.
    Bytes(Vec<u8>),
    /// Chữ ký của phần tử + các phần tử (dict = mảng các `Struct` 2 phần tử, chữ ký "{..}").
    Array(String, Vec<V>),
    Struct(Vec<V>),
    Variant(Box<V>),
}

impl V {
    pub fn s(s: &str) -> V {
        V::Str(s.to_string())
    }
    pub fn var(v: V) -> V {
        V::Variant(Box::new(v))
    }
    pub fn empty_dict() -> V {
        V::Array("{sv}".into(), Vec::new())
    }

    pub fn signature(&self, out: &mut String) {
        match self {
            V::Byte(_) => out.push('y'),
            V::Bool(_) => out.push('b'),
            V::I16(_) => out.push('n'),
            V::U16(_) => out.push('q'),
            V::I32(_) => out.push('i'),
            V::U32(_) => out.push('u'),
            V::I64(_) => out.push('x'),
            V::U64(_) => out.push('t'),
            V::F64(_) => out.push('d'),
            V::Str(_) => out.push('s'),
            V::Path(_) => out.push('o'),
            V::Sig(_) => out.push('g'),
            V::Bytes(_) => out.push_str("ay"),
            V::Array(e, _) => {
                out.push('a');
                out.push_str(e);
            }
            V::Struct(f) => {
                out.push('(');
                for x in f {
                    x.signature(out);
                }
                out.push(')');
            }
            V::Variant(_) => out.push('v'),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            V::Str(s) | V::Path(s) | V::Sig(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            V::U32(x) => Some(*x),
            _ => None,
        }
    }
    /// Bỏ các lớp variant bọc ngoài.
    pub fn inner(&self) -> &V {
        match self {
            V::Variant(v) => v.inner(),
            v => v,
        }
    }
}

// ---------- mã hoá ----------

fn pad(buf: &mut Vec<u8>, align: usize) {
    while buf.len() % align != 0 {
        buf.push(0);
    }
}

fn align_of(sig: u8) -> usize {
    match sig {
        b'y' | b'g' | b'v' => 1,
        b'n' | b'q' => 2,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 4,
    }
}

fn put_u32(buf: &mut Vec<u8>, x: u32) {
    pad(buf, 4);
    buf.extend_from_slice(&x.to_le_bytes());
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    put_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
}

fn put_sig(buf: &mut Vec<u8>, s: &str) {
    buf.push(s.len() as u8);
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
}

/// Ghi `v` vào `buf`; vị trí căn lề tính từ đầu `buf` (đầu thông điệp hoặc đầu
/// phần thân, vì phần thân luôn bắt đầu ở vị trí chia hết cho 8).
pub fn encode(buf: &mut Vec<u8>, v: &V) {
    match v {
        V::Byte(x) => buf.push(*x),
        V::Bool(b) => put_u32(buf, *b as u32),
        V::I16(x) => {
            pad(buf, 2);
            buf.extend_from_slice(&x.to_le_bytes());
        }
        V::U16(x) => {
            pad(buf, 2);
            buf.extend_from_slice(&x.to_le_bytes());
        }
        V::I32(x) => put_u32(buf, *x as u32),
        V::U32(x) => put_u32(buf, *x),
        V::I64(x) => {
            pad(buf, 8);
            buf.extend_from_slice(&x.to_le_bytes());
        }
        V::U64(x) => {
            pad(buf, 8);
            buf.extend_from_slice(&x.to_le_bytes());
        }
        V::F64(x) => {
            pad(buf, 8);
            buf.extend_from_slice(&x.to_le_bytes());
        }
        V::Str(s) | V::Path(s) => put_str(buf, s),
        V::Sig(s) => put_sig(buf, s),
        V::Bytes(b) => {
            put_u32(buf, b.len() as u32);
            buf.extend_from_slice(b);
        }
        V::Array(elem, items) => {
            put_u32(buf, 0);
            let len_at = buf.len() - 4;
            pad(buf, align_of(elem.as_bytes()[0]));
            let start = buf.len();
            for it in items {
                encode(buf, it);
            }
            let n = (buf.len() - start) as u32;
            buf[len_at..len_at + 4].copy_from_slice(&n.to_le_bytes());
        }
        V::Struct(fields) => {
            pad(buf, 8);
            for f in fields {
                encode(buf, f);
            }
        }
        V::Variant(inner) => {
            let mut sig = String::new();
            inner.signature(&mut sig);
            put_sig(buf, &sig);
            encode(buf, inner);
        }
    }
}

// ---------- giải mã ----------

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

fn bad() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "thông điệp D-Bus hỏng")
}

impl Reader<'_> {
    fn align(&mut self, a: usize) -> io::Result<()> {
        self.pos = self.pos.div_ceil(a) * a;
        if self.pos > self.b.len() {
            return Err(bad());
        }
        Ok(())
    }
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(bad)?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn fixed<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        self.align(N)?;
        Ok(self.take(N)?.try_into().unwrap())
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.fixed()?))
    }
    fn string(&mut self) -> io::Result<String> {
        let n = self.u32()? as usize;
        let s = String::from_utf8_lossy(self.take(n)?).into_owned();
        self.take(1)?;
        Ok(s)
    }
    fn sig(&mut self) -> io::Result<String> {
        let n = self.take(1)?[0] as usize;
        let s = String::from_utf8_lossy(self.take(n)?).into_owned();
        self.take(1)?;
        Ok(s)
    }

    /// Đọc một giá trị theo chữ ký bắt đầu tại `sig[*i]`, tăng `*i` qua kiểu đó.
    fn value(&mut self, sig: &[u8], i: &mut usize, depth: u32) -> io::Result<V> {
        if depth > 32 {
            return Err(bad());
        }
        let c = *sig.get(*i).ok_or_else(bad)?;
        *i += 1;
        Ok(match c {
            b'y' => V::Byte(self.take(1)?[0]),
            b'b' => V::Bool(self.u32()? != 0),
            b'n' => V::I16(i16::from_le_bytes(self.fixed()?)),
            b'q' => V::U16(u16::from_le_bytes(self.fixed()?)),
            b'i' => V::I32(self.u32()? as i32),
            b'u' => V::U32(self.u32()?),
            b'h' => V::U32(self.u32()?),
            b'x' => V::I64(i64::from_le_bytes(self.fixed()?)),
            b't' => V::U64(u64::from_le_bytes(self.fixed()?)),
            b'd' => V::F64(f64::from_le_bytes(self.fixed()?)),
            b's' => V::Str(self.string()?),
            b'o' => V::Path(self.string()?),
            b'g' => V::Sig(self.sig()?),
            b'v' => {
                let s = self.sig()?;
                let mut j = 0;
                let v = self.value(s.as_bytes(), &mut j, depth + 1)?;
                V::Variant(Box::new(v))
            }
            b'a' => {
                let n = self.u32()? as usize;
                let elem_start = *i;
                skip_type(sig, i)?;
                let elem = &sig[elem_start..*i];
                if elem == b"y" {
                    return Ok(V::Bytes(self.take(n)?.to_vec()));
                }
                self.align(align_of(elem[0]))?;
                let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(bad)?;
                let mut items = Vec::new();
                while self.pos < end {
                    let mut j = 0;
                    items.push(self.value(elem, &mut j, depth + 1)?);
                }
                V::Array(String::from_utf8_lossy(elem).into_owned(), items)
            }
            b'(' | b'{' => {
                self.align(8)?;
                let close = if c == b'(' { b')' } else { b'}' };
                let mut fields = Vec::new();
                while *sig.get(*i).ok_or_else(bad)? != close {
                    fields.push(self.value(sig, i, depth + 1)?);
                }
                *i += 1;
                V::Struct(fields)
            }
            _ => return Err(bad()),
        })
    }
}

/// Bỏ qua một kiểu hoàn chỉnh trong chữ ký.
fn skip_type(sig: &[u8], i: &mut usize) -> io::Result<()> {
    let c = *sig.get(*i).ok_or_else(bad)?;
    *i += 1;
    match c {
        b'a' => skip_type(sig, i),
        b'(' | b'{' => {
            let close = if c == b'(' { b')' } else { b'}' };
            while *sig.get(*i).ok_or_else(bad)? != close {
                skip_type(sig, i)?;
            }
            *i += 1;
            Ok(())
        }
        _ => Ok(()),
    }
}

// ---------- thông điệp ----------

#[derive(Debug, Default)]
pub struct Msg {
    pub kind: u8,
    pub flags: u8,
    pub serial: u32,
    pub path: String,
    pub iface: String,
    pub member: String,
    pub error_name: String,
    pub reply_serial: u32,
    pub dest: String,
    pub sender: String,
    pub body: Vec<V>,
}

impl Msg {
    pub fn arg_str(&self, i: usize) -> &str {
        self.body.get(i).and_then(V::as_str).unwrap_or("")
    }
    pub fn arg_u32(&self, i: usize) -> u32 {
        self.body.get(i).and_then(V::as_u32).unwrap_or(0)
    }
}

/// Tách một thông điệp hoàn chỉnh từ đầu `buf`. Trả về (thông điệp, số byte) hoặc None nếu chưa đủ.
fn parse(buf: &[u8]) -> io::Result<Option<(Msg, usize)>> {
    if buf.len() < 16 {
        return Ok(None);
    }
    if buf[0] != b'l' {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "chỉ hỗ trợ D-Bus little-endian"));
    }
    let body_len = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
    let fields_len = u32::from_le_bytes(buf[12..16].try_into().unwrap()) as usize;
    let header_end = (16 + fields_len).div_ceil(8) * 8;
    let total = header_end + body_len;
    if total > 64 << 20 {
        return Err(bad());
    }
    if buf.len() < total {
        return Ok(None);
    }
    let b = &buf[..total];
    let mut m = Msg {
        kind: b[1],
        flags: b[2],
        serial: u32::from_le_bytes(b[8..12].try_into().unwrap()),
        ..Default::default()
    };
    let mut r = Reader { b: &b[..16 + fields_len], pos: 16 };
    let mut signature = String::new();
    while r.pos < 16 + fields_len {
        r.align(8)?;
        let code = r.take(1)?[0];
        let s = r.sig()?;
        let mut j = 0;
        let v = r.value(s.as_bytes(), &mut j, 0)?;
        let text = v.as_str().unwrap_or("").to_string();
        match code {
            1 => m.path = text,
            2 => m.iface = text,
            3 => m.member = text,
            4 => m.error_name = text,
            5 => m.reply_serial = v.as_u32().unwrap_or(0),
            6 => m.dest = text,
            7 => m.sender = text,
            8 => signature = text,
            _ => {}
        }
    }
    let mut r = Reader { b: &b[header_end..], pos: 0 };
    let sig = signature.as_bytes();
    let mut i = 0;
    while i < sig.len() {
        m.body.push(r.value(sig, &mut i, 0)?);
    }
    Ok(Some((m, total)))
}

pub struct Out<'a> {
    pub kind: u8,
    pub flags: u8,
    pub path: &'a str,
    pub iface: &'a str,
    pub member: &'a str,
    pub error_name: &'a str,
    pub reply_serial: u32,
    pub dest: &'a str,
    pub body: &'a [V],
}

impl Default for Out<'_> {
    fn default() -> Self {
        Out {
            kind: METHOD_CALL,
            flags: 0,
            path: "",
            iface: "",
            member: "",
            error_name: "",
            reply_serial: 0,
            dest: "",
            body: &[],
        }
    }
}

fn serialize(o: &Out, serial: u32) -> Vec<u8> {
    let mut body = Vec::new();
    let mut sig = String::new();
    for v in o.body {
        v.signature(&mut sig);
        encode(&mut body, v);
    }
    let mut fields = Vec::new();
    let mut field = |code: u8, v: V| fields.push(V::Struct(vec![V::Byte(code), V::var(v)]));
    if !o.path.is_empty() {
        field(1, V::Path(o.path.into()));
    }
    if !o.iface.is_empty() {
        field(2, V::s(o.iface));
    }
    if !o.member.is_empty() {
        field(3, V::s(o.member));
    }
    if !o.error_name.is_empty() {
        field(4, V::s(o.error_name));
    }
    if o.reply_serial != 0 {
        field(5, V::U32(o.reply_serial));
    }
    if !o.dest.is_empty() {
        field(6, V::s(o.dest));
    }
    if !sig.is_empty() {
        field(8, V::Sig(sig));
    }
    let mut msg = vec![b'l', o.kind, o.flags, 1];
    msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
    msg.extend_from_slice(&serial.to_le_bytes());
    encode(&mut msg, &V::Array("(yv)".into(), fields));
    pad(&mut msg, 8);
    msg.extend_from_slice(&body);
    msg
}

pub struct Conn {
    sock: UnixStream,
    buf: Vec<u8>,
    serial: u32,
    pub unique: String,
    pending: VecDeque<Msg>,
}

fn session_socket() -> io::Result<UnixStream> {
    let addr = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| {
        let rt = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".into());
        format!("unix:path={rt}/bus")
    });
    let mut last = io::Error::new(io::ErrorKind::NotFound, "không có địa chỉ session bus");
    for part in addr.split(';') {
        let Some(params) = part.strip_prefix("unix:") else { continue };
        for kv in params.split(',') {
            let res = if let Some(p) = kv.strip_prefix("path=") {
                UnixStream::connect(unescape(p))
            } else if let Some(p) = kv.strip_prefix("abstract=") {
                use std::os::linux::net::SocketAddrExt;
                std::os::unix::net::SocketAddr::from_abstract_name(unescape(p).as_bytes())
                    .and_then(|a| UnixStream::connect_addr(&a))
            } else {
                continue;
            };
            match res {
                Ok(s) => return Ok(s),
                Err(e) => last = e,
            }
        }
    }
    Err(last)
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Conn {
    pub fn session() -> io::Result<Conn> {
        use std::os::unix::fs::MetadataExt;
        let mut sock = session_socket()?;
        let uid = std::fs::metadata("/proc/self")?.uid();
        let hex: String = uid.to_string().bytes().map(|b| format!("{b:02x}")).collect();
        sock.write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while !line.ends_with(b"\r\n") {
            if sock.read(&mut byte)? == 0 || line.len() > 512 {
                return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "D-Bus đóng khi xác thực"));
            }
            line.push(byte[0]);
        }
        if !line.starts_with(b"OK ") {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "D-Bus từ chối xác thực"));
        }
        sock.write_all(b"BEGIN\r\n")?;
        let mut c = Conn { sock, buf: Vec::new(), serial: 0, unique: String::new(), pending: VecDeque::new() };
        let r = c.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "Hello", &[])?;
        c.unique = r.arg_str(0).to_string();
        Ok(c)
    }

    pub fn send(&mut self, o: &Out) -> io::Result<u32> {
        self.serial = self.serial.wrapping_add(1).max(1);
        let bytes = serialize(o, self.serial);
        self.sock.write_all(&bytes)?;
        Ok(self.serial)
    }

    /// Gọi hàm và chờ trả lời (chỉ dùng lúc khởi động, khi chưa phục vụ ai).
    pub fn call(&mut self, dest: &str, path: &str, iface: &str, member: &str, body: &[V]) -> io::Result<Msg> {
        let serial = self.send(&Out { dest, path, iface, member, body, ..Default::default() })?;
        let mut later = Vec::new();
        let res = loop {
            match self.read_raw(None) {
                Ok(Some(m)) if m.reply_serial == serial && (m.kind == METHOD_RETURN || m.kind == ERROR) => {
                    break if m.kind == ERROR {
                        Err(io::Error::other(format!("{}: {}", m.error_name, m.arg_str(0))))
                    } else {
                        Ok(m)
                    };
                }
                Ok(Some(m)) => later.push(m),
                Ok(None) => {}
                Err(e) => break Err(e),
            }
        };
        self.pending.extend(later);
        res
    }

    /// Gọi hàm, không chờ trả lời.
    pub fn call_noreply(&mut self, dest: &str, path: &str, iface: &str, member: &str, body: &[V]) -> io::Result<()> {
        self.send(&Out { dest, path, iface, member, body, flags: NO_REPLY_EXPECTED, ..Default::default() })?;
        Ok(())
    }

    pub fn reply(&mut self, to: &Msg, body: &[V]) -> io::Result<()> {
        if to.flags & NO_REPLY_EXPECTED != 0 {
            return Ok(());
        }
        self.send(&Out { kind: METHOD_RETURN, reply_serial: to.serial, dest: &to.sender, body, ..Default::default() })?;
        Ok(())
    }

    pub fn error(&mut self, to: &Msg, name: &str, text: &str) -> io::Result<()> {
        if to.flags & NO_REPLY_EXPECTED != 0 {
            return Ok(());
        }
        self.send(&Out {
            kind: ERROR,
            error_name: name,
            reply_serial: to.serial,
            dest: &to.sender,
            body: &[V::s(text)],
            ..Default::default()
        })?;
        Ok(())
    }

    pub fn signal(&mut self, dest: &str, path: &str, iface: &str, member: &str, body: &[V]) -> io::Result<()> {
        self.send(&Out { kind: SIGNAL, dest, path, iface, member, body, ..Default::default() })?;
        Ok(())
    }

    /// Đọc thông điệp tiếp theo; None nếu hết `timeout` mà chưa có gì.
    pub fn read(&mut self, timeout: Option<Duration>) -> io::Result<Option<Msg>> {
        if let Some(m) = self.pending.pop_front() {
            return Ok(Some(m));
        }
        self.read_raw(timeout)
    }

    fn read_raw(&mut self, timeout: Option<Duration>) -> io::Result<Option<Msg>> {
        self.sock.set_read_timeout(timeout)?;
        loop {
            if let Some((m, n)) = parse(&self.buf)? {
                self.buf.drain(..n);
                if self.buf.is_empty() && self.buf.capacity() > 64 * 1024 {
                    self.buf = Vec::new();
                }
                return Ok(Some(m));
            }
            let mut chunk = [0u8; 4096];
            match self.sock.read(&mut chunk) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "D-Bus đã đóng kết nối")),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => return Ok(None),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let text = V::var(V::Struct(vec![
            V::s("IBusText"),
            V::empty_dict(),
            V::s("việt"),
            V::var(V::Struct(vec![V::s("IBusAttrList"), V::empty_dict(), V::Array("v".into(), vec![])])),
        ]));
        let body = vec![text.clone(), V::U32(3), V::Bool(true), V::Array("(iiay)".into(), vec![V::Struct(vec![
            V::I32(2),
            V::I32(1),
            V::Bytes(vec![1, 2, 3]),
        ])])];
        let bytes = serialize(
            &Out { kind: SIGNAL, path: "/a/b", iface: "x.y", member: "Z", dest: ":1.5", body: &body, ..Default::default() },
            7,
        );
        let (m, n) = parse(&bytes).unwrap().unwrap();
        assert_eq!(n, bytes.len());
        assert_eq!((m.kind, m.serial, m.path.as_str(), m.member.as_str()), (SIGNAL, 7, "/a/b", "Z"));
        assert_eq!(m.body, body);
        assert!(parse(&bytes[..bytes.len() - 1]).unwrap().is_none());
    }

    #[test]
    fn unescapes_address() {
        assert_eq!(unescape("/run/user/1000/bus"), "/run/user/1000/bus");
        assert_eq!(unescape("/tmp/a%2cb"), "/tmp/a,b");
    }
}
