//! Logic gõ tiếng Việt.
//!
//! Mỗi từ đang gõ được giữ ở hai dạng:
//! - `raw`: các phím đã bấm (dùng để trả lại nguyên văn khi từ không phải
//!   tiếng Việt hợp lệ, ví dụ gõ tiếng Anh "class", "text").
//! - `word`: danh sách chữ cái kèm dấu mũ/móc/trăng/gạch và một thanh điệu.
//!   Thanh điệu được đặt lại vị trí mỗi lần hiển thị, nên gõ dấu ở đâu cũng
//!   được ("vieetj" hay "vieejt" đều ra "việt").

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mark {
    None,
    Hat,    // â ê ô
    Breve,  // ă
    Horn,   // ơ ư
    Stroke, // đ
}

/// Thứ tự trùng với thứ tự ký tự trong bảng `compose`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tone {
    None = 0,
    Acute = 1, // sắc
    Grave = 2, // huyền
    Hook = 3,  // hỏi
    Tilde = 4, // ngã
    Dot = 5,   // nặng
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Letter {
    base: char, // chữ thường ASCII
    upper: bool,
    mark: Mark,
    /// "ư" sinh ra từ phím w đứng riêng (Telex); gõ w lần nữa thì thành "w".
    from_w: bool,
}

impl Letter {
    fn lit(c: char) -> Self {
        Letter {
            base: c.to_ascii_lowercase(),
            upper: c.is_uppercase(),
            mark: Mark::None,
            from_w: false,
        }
    }
    fn is_vowel(&self) -> bool {
        matches!(self.base, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
    }
    fn render(&self, tone: Tone) -> char {
        let c = compose(self.base, self.mark, tone);
        if self.upper {
            c.to_uppercase().next().unwrap_or(c)
        } else {
            c
        }
    }
}

fn compose(base: char, mark: Mark, tone: Tone) -> char {
    let row = match (base, mark) {
        ('a', Mark::Breve) => "ăắằẳẵặ",
        ('a', Mark::Hat) => "âấầẩẫậ",
        ('a', _) => "aáàảãạ",
        ('e', Mark::Hat) => "êếềểễệ",
        ('e', _) => "eéèẻẽẹ",
        ('i', _) => "iíìỉĩị",
        ('o', Mark::Hat) => "ôốồổỗộ",
        ('o', Mark::Horn) => "ơớờởỡợ",
        ('o', _) => "oóòỏõọ",
        ('u', Mark::Horn) => "ưứừửữự",
        ('u', _) => "uúùủũụ",
        ('y', _) => "yýỳỷỹỵ",
        ('d', Mark::Stroke) => return 'đ',
        _ => return base,
    };
    row.chars().nth(tone as usize).unwrap_or(base)
}

enum Action {
    Letter,
    Tone(Tone),
    ClearTone,
    /// Dấu mũ: chỉ đúng nguyên âm trùng phím (aa, ee, oo).
    Hat(char),
    /// w: ư/ơ/ă, "uo" -> "ươ", w đứng riêng -> ư.
    TelexW,
    Stroke,
}

fn action(k: char) -> Action {
    match k.to_ascii_lowercase() {
        's' => Action::Tone(Tone::Acute),
        'f' => Action::Tone(Tone::Grave),
        'r' => Action::Tone(Tone::Hook),
        'x' => Action::Tone(Tone::Tilde),
        'j' => Action::Tone(Tone::Dot),
        'z' => Action::ClearTone,
        l @ ('a' | 'e' | 'o') => Action::Hat(l),
        'w' => Action::TelexW,
        'd' => Action::Stroke,
        _ => Action::Letter,
    }
}

const INITIALS: &[&str] = &[
    "", "b", "c", "ch", "d", "đ", "g", "gh", "gi", "h", "k", "kh", "l", "m", "n", "ng", "ngh",
    "nh", "p", "ph", "qu", "r", "s", "t", "th", "tr", "v", "x",
];

/// Phần vần (chỉ xét chữ gốc, bỏ dấu mũ/móc).
const NUCLEI: &[&str] = &[
    "a", "e", "i", "o", "u", "y", "ai", "ao", "au", "ay", "eo", "eu", "ia", "ie", "iu", "oa", "oe",
    "oi", "oo", "ua", "ue", "ui", "uo", "uu", "uy", "ye", "ieu", "oai", "oao", "oay", "oeo", "uay",
    "uoi", "uou", "uya", "uye", "uyu", "yeu",
];

// "k" cho địa danh như Đắk Lắk.
const FINALS: &[&str] = &["", "c", "ch", "k", "m", "n", "ng", "nh", "p", "t"];

/// Các vần có dấu mũ hợp lệ (để không biến "khoeo" thành "khôe").
const HAT_NUCLEI: &[&str] = &[
    "â", "âu", "ây", "ê", "êu", "iê", "iêu", "ô", "ôi", "uâ", "uây", "uê", "uô", "uôi", "yê", "yêu", "uyê",
];

/// Các vần có dấu móc/trăng hợp lệ.
const HORN_NUCLEI: &[&str] = &[
    "ă", "ơ", "ư", "oă", "ơi", "ưa", "ưi", "ưu", "uơ", "ươ", "ươi", "ươu",
];

#[derive(Clone, Debug)]
struct Word {
    letters: Vec<Letter>,
    tone: Tone,
    /// Phím vừa gõ chỉ để bỏ dấu (gõ lặp: "ss", "aaa", "ww"), giống Unikey:
    /// phím này không được tính khi khôi phục từ sai ("puss" -> "pus").
    undone: bool,
    /// Phím vừa dùng để bỏ dấu: gõ tiếp đúng phím đó thì giữ nguyên chữ
    /// ("www" -> "ww", "asss" -> "ass"), giống Unikey.
    escaped: Option<char>,
}

impl Word {
    fn new() -> Self {
        Word {
            letters: Vec::new(),
            tone: Tone::None,
            undone: false,
            escaped: None,
        }
    }

    fn replay(keys: &[char]) -> Self {
        let mut w = Word::new();
        for &k in keys {
            w.apply(k);
        }
        w
    }

    fn push(&mut self, k: char) {
        self.letters.push(Letter::lit(k));
    }

    /// Khoảng [start, end) của cụm nguyên âm chính, đã bỏ "u" trong "qu"
    /// và "i" trong "gi" khi chúng đóng vai phụ âm đầu.
    fn vowel_range(&self) -> Option<(usize, usize)> {
        let l = &self.letters;
        let n = l.len();
        let mut s = l.iter().position(|x| x.is_vowel())?;
        if s == 1 && s + 1 < n && l[s + 1].is_vowel() {
            let skip = (l[0].base == 'q' && l[1].base == 'u')
                || (l[0].base == 'g' && l[1].base == 'i' && l[1].mark == Mark::None);
            if skip {
                s += 1;
            }
        }
        let mut e = s;
        while e < n && l[e].is_vowel() {
            e += 1;
        }
        Some((s, e))
    }

    fn apply(&mut self, k: char) {
        let lk = k.to_ascii_lowercase();
        let act = action(k);
        if self.escaped == Some(lk) && !matches!(act, Action::Letter) {
            self.push(k);
            return;
        }
        self.escaped = None;
        self.apply_action(act, k);
        if self.undone {
            self.escaped = Some(lk);
        }
        self.fix_uo();
    }

    /// "uơ"/"ưo" đứng trước chữ khác thì phải là "ươ": duowng -> dương,
    /// uwoi -> ươi, huwou -> hươu. ("thuở", "huơ" không có chữ phía sau nên giữ nguyên.)
    fn fix_uo(&mut self) {
        let Some((s, e)) = self.vowel_range() else { return };
        for i in s..e.saturating_sub(1) {
            let (u, o) = (self.letters[i], self.letters[i + 1]);
            // Chỉ khi theo sau là i/u hoặc âm cuối (c, m, n, p, t...), không phải phím thừa như "uơf".
            let next_ok = self
                .letters
                .get(i + 2)
                .is_some_and(|n| matches!(n.base, 'i' | 'u' | 'c' | 'm' | 'n' | 'p' | 't'));
            if u.base != 'u' || o.base != 'o' || !next_ok {
                continue;
            }
            match (u.mark, o.mark) {
                (Mark::None, Mark::Horn) => self.letters[i].mark = Mark::Horn,
                (Mark::Horn, Mark::None) => self.letters[i + 1].mark = Mark::Horn,
                _ => {}
            }
        }
    }

    /// Vần (có dấu mũ/móc, chưa có thanh) nếu đặt `mark` cho chữ thứ `idx`.
    fn nucleus_with(&self, s: usize, e: usize, idx: usize, mark: Mark) -> String {
        (s..e)
            .map(|i| {
                let l = self.letters[i];
                compose(l.base, if i == idx { mark } else { l.mark }, Tone::None)
            })
            .collect()
    }

    fn apply_action(&mut self, act: Action, k: char) {
        match act {
            Action::Letter => self.push(k),
            Action::Tone(t) => {
                if self.vowel_range().is_none() {
                    self.push(k);
                } else if self.tone == t {
                    // Gõ lại cùng dấu: bỏ dấu, giữ nguyên phím ("ass" -> "as").
                    self.tone = Tone::None;
                    self.push(k);
                    self.undone = true;
                } else {
                    self.tone = t;
                }
            }
            Action::ClearTone => {
                if self.tone != Tone::None {
                    self.tone = Tone::None;
                } else {
                    self.push(k);
                }
            }
            Action::Hat(target) => {
                if let Some((s, e)) = self.vowel_range() {
                    let found = (s..e).rev().find(|&i| self.letters[i].base == target);
                    if let Some(i) = found {
                        if self.letters[i].mark == Mark::Hat {
                            self.letters[i].mark = Mark::None;
                            self.push(k);
                            self.undone = true;
                        } else if HAT_NUCLEI.contains(&self.nucleus_with(s, e, i, Mark::Hat).as_str()) {
                            self.letters[i].mark = Mark::Hat;
                        } else {
                            self.push(k);
                        }
                        return;
                    }
                }
                self.push(k);
            }
            Action::TelexW => {
                if self.letters.last().is_some_and(|l| l.from_w) {
                    // "ww" -> "w"
                    let i = self.letters.len() - 1;
                    self.letters[i] = Letter::lit(k);
                    self.undone = true;
                    return;
                }
                if !self.horn(k) {
                    // w đứng riêng thành ư, trừ khi đã có nguyên âm hoặc đứng sau q ("qw").
                    let standalone = self.vowel_range().is_none()
                        && !self.letters.last().is_some_and(|l| l.base == 'q');
                    if standalone {
                        self.letters.push(Letter {
                            base: 'u',
                            upper: k.is_uppercase(),
                            mark: Mark::Horn,
                            from_w: true,
                        });
                    } else {
                        self.push(k);
                    }
                }
            }
            Action::Stroke => {
                if self.letters.first().is_some_and(|l| l.base == 'd') {
                    self.toggle(0, Mark::Stroke, k);
                } else {
                    self.push(k);
                }
            }
        }
    }

    /// Đặt dấu `mark` cho chữ thứ `i`; nếu đã có thì gỡ và thêm phím vào.
    fn toggle(&mut self, i: usize, mark: Mark, k: char) {
        if self.letters[i].mark == mark {
            self.letters[i].mark = Mark::None;
            self.push(k);
            self.undone = true;
        } else {
            self.letters[i].mark = mark;
        }
    }

    /// Dấu móc cho u/o, dấu trăng cho a. Trả về false nếu không có nguyên âm
    /// nào nhận dấu.
    fn horn(&mut self, k: char) -> bool {
        let Some((s, e)) = self.vowel_range() else {
            return false;
        };
        for i in s..e.saturating_sub(1) {
            if self.letters[i].base == 'u' && self.letters[i + 1].base == 'o' {
                // Giống Unikey: uow -> uơ, uoww -> ươ, uowww -> uow.
                match (self.letters[i].mark, self.letters[i + 1].mark) {
                    (Mark::Horn, Mark::Horn) => {
                        self.letters[i].mark = Mark::None;
                        self.letters[i + 1].mark = Mark::None;
                        self.push(k);
                        self.undone = true;
                    }
                    (_, Mark::Horn) => self.letters[i].mark = Mark::Horn,
                    _ => self.letters[i + 1].mark = Mark::Horn,
                }
                return true;
            }
        }
        let mark_of = |b: char| if b == 'a' { Mark::Breve } else { Mark::Horn };
        // Các chữ có thể nhận dấu, từ cuối lên.
        let cands: Vec<usize> = (s..e)
            .rev()
            .filter(|&i| {
                let b = self.letters[i].base;
                // "qu": u là phụ âm, không nhận móc ("quw" giữ nguyên).
                let after_q = b == 'u' && i > 0 && self.letters[i - 1].base == 'q';
                (b == 'u' && !after_q) || b == 'o' || b == 'a'
            })
            .collect();
        // Ưu tiên chữ cho vần hợp lệ ("uuw" -> "ưu", "muaw" -> "mưa"); nếu không
        // có thì gỡ dấu đang có ("ưuw" -> "uuw"); cuối cùng là chữ gần cuối nhất.
        let pick = cands
            .iter()
            .copied()
            .find(|&i| {
                let m = mark_of(self.letters[i].base);
                self.letters[i].mark != m && HORN_NUCLEI.contains(&self.nucleus_with(s, e, i, m).as_str())
            })
            .or_else(|| cands.iter().copied().find(|&i| self.letters[i].mark == mark_of(self.letters[i].base)))
            .or(cands.first().copied());
        match pick {
            Some(i) => {
                self.toggle(i, mark_of(self.letters[i].base), k);
                true
            }
            None => false,
        }
    }

    fn tone_index(&self, modern: bool) -> Option<usize> {
        let (s, e) = self.vowel_range()?;
        let l = &self.letters;
        for i in s..e.saturating_sub(1) {
            if l[i].base == 'u' && l[i].mark == Mark::Horn && l[i + 1].mark == Mark::Horn {
                return Some(i + 1); // ươ: dấu trên ơ
            }
        }
        if let Some(i) = (s..e)
            .rev()
            .find(|&i| matches!(l[i].mark, Mark::Hat | Mark::Breve | Mark::Horn))
        {
            return Some(i);
        }
        let has_final = e < l.len();
        Some(match e - s {
            1 => s,
            2 => {
                let pair = (l[s].base, l[s + 1].base);
                let new_style = matches!(pair, ('o', 'a') | ('o', 'e') | ('u', 'y'));
                if has_final || (modern && new_style) {
                    s + 1
                } else {
                    s
                }
            }
            _ => s + 1,
        })
    }

    fn render(&self, modern: bool) -> String {
        let ti = if self.tone == Tone::None {
            None
        } else {
            self.tone_index(modern)
        };
        self.letters
            .iter()
            .enumerate()
            .map(|(i, l)| l.render(if Some(i) == ti { self.tone } else { Tone::None }))
            .collect()
    }

    /// Kiểm tra cấu trúc âm tiết tiếng Việt (phụ âm đầu + vần + âm cuối).
    fn is_valid(&self) -> bool {
        let l = &self.letters;
        if l.iter().any(|x| !x.base.is_ascii_lowercase()) {
            return false;
        }
        let Some((s, e)) = self.vowel_range() else {
            return self.tone == Tone::None;
        };
        let initial: String = l[..s].iter().map(|x| compose(x.base, x.mark, Tone::None)).collect();
        let nucleus: String = l[s..e].iter().map(|x| x.base).collect();
        let fin: String = l[e..].iter().map(|x| x.base).collect();
        if !INITIALS.contains(&initial.as_str())
            || !NUCLEI.contains(&nucleus.as_str())
            || !FINALS.contains(&fin.as_str())
        {
            return false;
        }
        if matches!(fin.as_str(), "c" | "ch" | "p" | "t")
            && !matches!(self.tone, Tone::None | Tone::Acute | Tone::Dot)
        {
            return false;
        }
        true
    }

    /// Chuyển từ hiện tại thành chuỗi phím chuẩn (dùng sau khi xoá lùi).
    fn encode(&self) -> Vec<char> {
        let mut out = Vec::new();
        for l in &self.letters {
            out.push(if l.upper { l.base.to_ascii_uppercase() } else { l.base });
            match l.mark {
                Mark::None => {}
                Mark::Hat => out.push(l.base),
                Mark::Breve | Mark::Horn => out.push('w'),
                Mark::Stroke => out.push('d'),
            }
        }
        if self.tone != Tone::None {
            out.push(['s', 'f', 'r', 'x', 'j'][self.tone as usize - 1]);
        }
        out
    }
}

/// Bộ gõ Telex cho một ô nhập liệu: giữ từ đang gõ dở.
#[derive(Clone, Debug)]
pub struct Engine {
    modern_tone: bool,
    restore_invalid: bool,
    raw: Vec<char>,
    word: Word,
    /// Từ vừa kết thúc bằng dấu cách/dấu câu và số ký tự ngăn cách gõ sau nó:
    /// xoá lùi hết các ký tự đó thì gõ tiếp được vào từ cũ (thêm dấu), giống Unikey.
    prev: Option<Box<(Vec<char>, Word)>>,
    gap: u32,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            modern_tone: false,
            restore_invalid: true,
            raw: Vec::new(),
            word: Word::new(),
            prev: None,
            gap: 0,
        }
    }

    /// true: đặt dấu kiểu mới ("hoà", "thuý"); false (mặc định): kiểu cũ ("hòa", "thúy").
    pub fn set_modern_tone(&mut self, modern: bool) {
        self.modern_tone = modern;
    }

    /// true (mặc định): từ không phải tiếng Việt thì trả lại đúng các phím đã gõ.
    pub fn set_restore_invalid(&mut self, restore: bool) {
        self.restore_invalid = restore;
    }

    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Quên hẳn từ đang gõ và từ trước đó (đổi chỗ con trỏ, Enter, phím tắt...).
    pub fn reset(&mut self) {
        self.raw.clear();
        self.word = Word::new();
        self.prev = None;
        self.gap = 0;
    }

    /// Đã gõ một ký tự ngăn cách (dấu cách, dấu câu) vào ứng dụng: kết thúc từ
    /// hiện tại nhưng vẫn nhớ nó để xoá lùi quay lại được.
    pub fn separator(&mut self) {
        if !self.raw.is_empty() {
            let raw = std::mem::take(&mut self.raw);
            let word = std::mem::replace(&mut self.word, Word::new());
            self.prev = Some(Box::new((raw, word)));
            self.gap = 1;
        } else if self.prev.is_some() {
            self.gap += 1;
        }
    }

    /// Đưa một phím vào từ đang gõ. Trả về false nếu phím không thuộc về từ
    /// (dấu cách, dấu câu...): khi đó bên gọi cần kết thúc từ hiện tại.
    pub fn process(&mut self, k: char) -> bool {
        if !k.is_ascii_alphabetic() {
            return false;
        }
        let repeat = self.raw.last().is_some_and(|c| c.eq_ignore_ascii_case(&k));
        self.word.undone = false;
        self.word.apply(k);
        // Phím lặp lại ngay để bỏ dấu thì không tính ("puss" -> "pus").
        if !(self.word.undone && repeat) {
            self.raw.push(k);
        }
        true
    }

    fn raw_string(&self) -> String {
        self.raw.iter().collect()
    }

    fn showing_raw(&self) -> bool {
        let rendered = self.word.render(self.modern_tone);
        self.restore_invalid && rendered != self.raw_string() && !self.word.is_valid()
    }

    /// Chuỗi hiển thị của từ đang gõ.
    pub fn text(&self) -> String {
        if self.showing_raw() {
            self.raw_string()
        } else {
            self.word.render(self.modern_tone)
        }
    }

    /// Xoá lùi một ký tự. Trả về true nếu ký tự đó thuộc từ đang gõ (bên gọi
    /// cập nhật lại từ). Trả về false nếu từ đang rỗng: phím Backspace được để
    /// cho ứng dụng xoá, và nếu nó xoá hết các ký tự ngăn cách sau từ trước thì
    /// từ trước trở thành từ đang gõ (`is_empty()` thành false).
    pub fn backspace(&mut self) -> bool {
        if self.raw.is_empty() {
            if self.prev.is_some() {
                self.gap = self.gap.saturating_sub(1);
                if self.gap == 0 {
                    let (raw, word) = *self.prev.take().unwrap();
                    self.raw = raw;
                    self.word = word;
                }
            }
            return false;
        }
        if self.showing_raw() {
            self.raw.pop();
            self.word = Word::replay(&self.raw);
        } else {
            self.word.letters.pop();
            if self.word.vowel_range().is_none() {
                self.word.tone = Tone::None;
            }
            self.raw = self.word.encode();
        }
        self.word.escaped = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn telex(keys: &str) -> String {
        let mut e = Engine::new();
        for k in keys.chars() {
            assert!(e.process(k), "phím {k:?} bị từ chối trong {keys:?}");
        }
        e.text()
    }

    #[test]
    fn telex_basic() {
        let cases = [
            ("vieejt", "việt"),
            ("vieetj", "việt"),
            ("tieengs", "tiếng"),
            ("nguowif", "người"),
            ("dduwowngf", "đường"),
            ("duwowngf", "dường"),
            ("dduongwf", "đường"),
            ("quyeets", "quyết"),
            ("gif", "gì"),
            ("giaf", "già"),
            ("gieets", "giết"),
            ("ruwowuj", "rượu"),
            ("ngoaif", "ngoài"),
            ("khuyur", "khuỷu"),
            ("hoaf", "hòa"),
            ("thuys", "thúy"),
            ("huyts", "huýt"),
            ("cuar", "của"),
            ("cuwar", "cửa"),
            ("maays", "mấy"),
            ("chaof", "chào"),
            ("xin", "xin"),
            ("aw", "ă"),
            ("awn", "ăn"),
            ("tooi", "tôi"),
            ("tono", "tôn"),
            ("w", "ư"),
            ("ww", "w"),
            ("tw", "tư"),
            ("ass", "as"),
            ("puss", "pus"),
            ("pussh", "push"),
            ("pass", "pas"),
            ("passs", "pass"),
            ("classs", "class"),
            ("thiss", "this"),
            ("russt", "rust"),
            ("aaa", "aa"),
            ("ddd", "dd"),
            ("asz", "a"),
            ("Vieejt", "Việt"),
            ("VIEEJT", "VIỆT"),
            ("DDaau", "Đâu"),
            ("thuowr", "thuở"),
            ("huow", "huơ"),
            ("thuoww", "thươ"),
            ("duowngf", "dường"),
            ("nguoiwf", "người"),
            ("uwoi", "ươi"),
            ("huwou", "hươu"),
            ("quoww", "quow"),
            ("qw", "qw"),
            ("quw", "quw"),
            ("www", "ww"),
            ("asss", "ass"),
            ("khoeo", "khoeo"),
            ("ngoeof", "ngoèo"),
            ("ddawks", "đắk"),
            ("Lawks", "Lắk"),
            ("khoer", "khỏe"),
            ("xoong", "xông"),
            ("xooong", "xoong"),
            ("nghieeng", "nghiêng"),
            ("uuw", "ưu"),
            ("huuw", "hưu"),
            ("cuuw", "cưu"),
            ("muaw", "mưa"),
            ("muwa", "mưa"),
            ("tuwuw", "tuwuw"),
            ("oaw", "oă"),
            ("xoawn", "xoăn"),
            ("quawng", "quăng"),
            ("hoiw", "hơi"),
            ("guiw", "gưi"),
        ];
        for (k, want) in cases {
            assert_eq!(telex(k), want, "telex {k}");
        }
    }

    #[test]
    fn telex_restores_english() {
        for w in ["text", "what", "window", "hello", "google", "linux", "push", "github"] {
            assert_eq!(telex(w), w, "telex {w}");
        }
    }

    #[test]
    fn no_restore() {
        let mut e = Engine::new();
        e.set_restore_invalid(false);
        for k in "text".chars() {
            e.process(k);
        }
        assert_eq!(e.text(), "tẽt");
    }

    #[test]
    fn modern_tone() {
        let mut e = Engine::new();
        e.set_modern_tone(true);
        for k in "hoaf".chars() {
            e.process(k);
        }
        assert_eq!(e.text(), "hoà");
    }

    #[test]
    fn non_letters_end_word() {
        let mut t = Engine::new();
        assert!(t.process('a'));
        assert!(!t.process('1'));
        assert!(!t.process(' '));
    }

    #[test]
    fn backspace() {
        let mut e = Engine::new();
        for k in "vieejt".chars() {
            e.process(k);
        }
        assert!(e.backspace());
        assert_eq!(e.text(), "việ");
        e.process('c');
        assert_eq!(e.text(), "việc");

        let mut e = Engine::new();
        for k in "classs".chars() {
            e.process(k);
        }
        assert_eq!(e.text(), "class");
        e.backspace();
        assert_eq!(e.text(), "clas");

        let mut e = Engine::new();
        e.process('a');
        assert!(e.backspace());
        assert!(e.is_empty());
        assert!(!e.backspace());
    }

    fn type_into(e: &mut Engine, app: &mut String, keys: &str) {
        for k in keys.chars() {
            if k == '<' {
                if !e.backspace() {
                    app.pop();
                    continue;
                }
            } else if e.process(k) {
            } else {
                e.separator();
                app.push(k);
                continue;
            }
            // Thay phần từ đang gõ trong app bằng chữ mới.
            let typed_len = app.chars().rev().take_while(|c| !" ,.".contains(*c)).count();
            for _ in 0..typed_len {
                app.pop();
            }
            app.push_str(&e.text());
        }
    }

    #[test]
    fn backspace_over_space_resumes_word() {
        // Gõ "nua ", xoá dấu cách, gõ thêm "wx" -> "nữa".
        let mut e = Engine::new();
        let mut app = String::new();
        type_into(&mut e, &mut app, "nua <");
        assert_eq!(app, "nua");
        assert!(!e.is_empty(), "xoá dấu cách thì quay lại từ cũ");
        type_into(&mut e, &mut app, "wx");
        assert_eq!(app, "nữa");

        // Nhiều dấu cách, và xoá cả từ sau.
        let mut e = Engine::new();
        let mut app = String::new();
        type_into(&mut e, &mut app, "vuong  ab<<<<f");
        assert_eq!(app, "vuòng");
        type_into(&mut e, &mut app, "w ");
        assert_eq!(app, "vường ");

        // Đã reset (Enter, bấm chuột) thì không quay lại.
        let mut e = Engine::new();
        let mut app = String::new();
        type_into(&mut e, &mut app, "nua ");
        e.reset();
        type_into(&mut e, &mut app, "<");
        assert!(e.is_empty());
        e.process('s');
        assert_eq!(e.text(), "s");
    }
}
