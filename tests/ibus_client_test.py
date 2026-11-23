"""Giả làm gnome-shell: dùng thư viện IBus thật (libibus) nói chuyện với hfastkey-daemon."""
import sys
import gi

gi.require_version("IBus", "1.0")
from gi.repository import GLib, IBus

IBus.init()
bus = IBus.Bus.new()
assert bus.is_connected(), "không kết nối được bus"

events = []
loop_ctx = GLib.MainContext.default()


def make_ic(client_commit):
    ic = bus.create_input_context("gnome-shell-test")
    assert ic is not None, "CreateInputContext lỗi"
    ic.set_capabilities(IBus.Capabilite.PREEDIT_TEXT | IBus.Capabilite.FOCUS)
    if client_commit:
        ic.set_client_commit_preedit(True)
    ic.connect("commit-text", lambda _c, t: events.append(("commit", t.get_text())))
    ic.connect("forward-key-event", lambda _c, kv, kc, st: events.append(("forward", hex(kv), "release" if st & IBus.ModifierType.RELEASE_MASK else "press")))
    ic.connect("update-preedit-text", lambda _c, t, p, v: events.append(("preedit", t.get_text())))
    ic.connect("update-preedit-text-with-mode", lambda _c, t, p, v, m: events.append(("preedit", t.get_text())))
    ic.focus_in()
    return ic


def key(ic, keyval, keycode=30, state=0):
    done = []

    def cb(ctx, res):
        done.append(ctx.process_key_event_async_finish(res))

    ic.process_key_event_async(keyval, keycode, state, -1, None, cb)
    while not done:
        loop_ctx.iteration(True)
    while loop_ctx.pending():
        loop_ctx.iteration(False)
    events.append(("reply", done[0]))
    return done[0]


def type_text(ic, text):
    for ch in text:
        st = IBus.ModifierType.SHIFT_MASK if ch.isupper() else 0
        key(ic, ord(ch), 30, st)
        key(ic, ord(ch), 30, st | IBus.ModifierType.RELEASE_MASK)


def commits():
    return [e[1] for e in events if e[0] == "commit"]


fail = 0


def check(name, cond, detail=""):
    global fail
    print(("OK   " if cond else "FAIL ") + name + (("  -> " + str(detail)) if not cond else ""))
    if not cond:
        fail += 1


for client_commit in (True, False):
    tag = "[client_commit=%s]" % client_commit
    ic = make_ic(client_commit)

    events.clear()
    type_text(ic, "xin chaof")
    events_before_enter = len(events)
    key(ic, IBus.KEY_Return, 28, 0)
    tail = events[events_before_enter:]
    check(tag + " gõ 'xin chaof' + Enter: chữ gửi đủ", "".join(commits()) == "xin chào", commits())
    order = [e[0] for e in tail if e[0] != "preedit"]
    check(tag + " thứ tự: commit -> forward(nhấn) -> forward(nhả) -> reply", order == ["commit", "forward", "forward", "reply"], tail)
    check(tag + " Enter gửi lại đủ nhấn + nhả", [e for e in tail if e[0] == "forward"] == [("forward", "0xff0d", "press"), ("forward", "0xff0d", "release")], tail)
    r = key(ic, IBus.KEY_Return, 28, IBus.ModifierType.RELEASE_MASK)
    check(tag + " nhả Enter thật bị nuốt", r is True)
    r = key(ic, IBus.KEY_Return, 28, 0)
    check(tag + " Enter khi không có chữ dở đi thẳng tới ứng dụng", r is False)
    key(ic, IBus.KEY_Return, 28, IBus.ModifierType.RELEASE_MASK)

    events.clear()
    type_text(ic, "Tieengs Vieejt nheS")
    key(ic, IBus.KEY_period, 52)
    check(tag + " câu có dấu", "".join(commits()) == "Tiếng Việt nhé.", commits())

    events.clear()
    type_text(ic, "classs")
    preedits = [e[1] for e in events if e[0] == "preedit"]
    check(tag + " gõ tiếng Anh 'class' giữ nguyên", preedits[-1] == "class", preedits)
    ic.focus_out()
    deadline = GLib.get_monotonic_time() + 300000
    while GLib.get_monotonic_time() < deadline:
        loop_ctx.iteration(False)
    if client_commit:
        check(tag + " mất focus: để ứng dụng tự chốt (không commit trùng)", commits() == [], commits())
    else:
        check(tag + " mất focus: bộ gõ chốt hộ", commits() == ["class"], commits())
    ic.focus_in()

    events.clear()
    key(ic, IBus.KEY_space, 57, IBus.ModifierType.CONTROL_MASK)
    key(ic, IBus.KEY_space, 57, IBus.ModifierType.CONTROL_MASK | IBus.ModifierType.RELEASE_MASK)
    r = key(ic, ord("a"), 30)
    check(tag + " Ctrl+Space tắt tiếng Việt", r is False)
    key(ic, IBus.KEY_space, 57, IBus.ModifierType.CONTROL_MASK)
    key(ic, IBus.KEY_space, 57, IBus.ModifierType.CONTROL_MASK | IBus.ModifierType.RELEASE_MASK)
    r = key(ic, ord("a"), 30)
    check(tag + " Ctrl+Space bật lại", r is True)
    ic.reset()
    ic.destroy()

# ---- Chế độ gõ thẳng (không gạch chân), giống Unikey ----
class FakeEntry:
    """Ô nhập liệu giả: áp dụng xoá/chèn chữ và báo lại chữ quanh con trỏ."""

    def __init__(self):
        self.text = ""
        self.preedits = []
        self.ic = bus.create_input_context("gnome-shell-test")
        self.ic.set_capabilities(IBus.Capabilite.PREEDIT_TEXT | IBus.Capabilite.FOCUS | IBus.Capabilite.SURROUNDING_TEXT)
        self.ic.set_client_commit_preedit(True)
        self.ic.connect("commit-text", lambda _c, t: self._commit(t.get_text()))
        self.ic.connect("delete-surrounding-text", lambda _c, off, n: self._delete(off, n))
        self.ic.connect("update-preedit-text-with-mode", lambda _c, t, p, v, m: self.preedits.append(t.get_text()) if t.get_text() else None)
        self.ic.focus_in()
        self.report()

    def _commit(self, t):
        self.text += t

    def _delete(self, off, n):
        assert off == -n, (off, n)
        self.text = self.text[: len(self.text) - n]

    def report(self):
        n = len(self.text)
        self.ic.set_surrounding_text(IBus.Text.new_from_string(self.text), n, n)
        deadline = GLib.get_monotonic_time() + 20000
        while GLib.get_monotonic_time() < deadline:
            loop_ctx.iteration(False)

    def press(self, keyval, keycode=30, state=0):
        handled = key(self.ic, keyval, keycode, state)
        if not handled and 0x20 <= keyval < 0x7F and not state & IBus.ModifierType.CONTROL_MASK:
            self.text += chr(keyval)
        self.report()
        return handled

    def type(self, s):
        for ch in s:
            st = IBus.ModifierType.SHIFT_MASK if ch.isupper() else 0
            self.press(ord(ch), 30, st)
            key(self.ic, ord(ch), 30, st | IBus.ModifierType.RELEASE_MASK)


e = FakeEntry()
e.type("xin chaof")
check("[gõ thẳng] chữ đi thẳng vào ô nhập", e.text == "xin chào", e.text)
check("[gõ thẳng] không dùng gạch chân", e.preedits == [], e.preedits)
r = e.press(IBus.KEY_Return, 28)
check("[gõ thẳng] Enter đi thẳng tới ứng dụng ngay lần đầu", r is False)
key(e.ic, IBus.KEY_Return, 28, IBus.ModifierType.RELEASE_MASK)
e.text = ""
e.report()
e.type("Tieengs Vieejt raast ddepj, nguowif ddaau?")
check("[gõ thẳng] câu dài có dấu", e.text == "Tiếng Việt rất đẹp, người đâu?", e.text)
e.text = ""
e.report()
e.type("vieejt")
e.press(IBus.KEY_BackSpace, 14)
e.type("c classs pussh")
check("[gõ thẳng] xoá lùi giữa từ + gõ tiếng Anh", e.text == "việc class push", e.text)
e.ic.destroy()

print("FAILED: %d" % fail if fail else "ALL OK")
sys.exit(1 if fail else 0)
