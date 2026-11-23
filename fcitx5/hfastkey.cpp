// Lớp nối mỏng giữa fcitx5 và lõi Rust. Toàn bộ logic tiếng Việt nằm trong
// lõi Rust; file này chỉ nhận phím, hiển thị chữ đang gõ và chốt chữ.
//
// Quy tắc quan trọng để không mất chữ trên GNOME Wayland:
// - Phím kết thúc từ là ký tự in được (cách, dấu câu): gửi "từ + ký tự" trong
//   một lần commit, nuốt phím gốc -> thứ tự luôn đúng.
// - Phím điều khiển (Enter, Tab, mũi tên, Ctrl+...): commit từ trước, rồi tự
//   gửi lại phím (cả nhấn và nhả), nuốt phím gốc và lần nhả của nó.

#include "hfastkey.h"

#include <fcitx-utils/key.h>
#include <fcitx-utils/keysym.h>
#include <fcitx/addonfactory.h>
#include <fcitx/addoninstance.h>
#include <fcitx/addonmanager.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextmanager.h>
#include <fcitx/inputcontextproperty.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/inputmethodentry.h>
#include <fcitx/inputpanel.h>
#include <fcitx/instance.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>
#include <string>

namespace fcitx {

namespace {
uint32_t methodFor(const InputMethodEntry &entry) {
    return entry.uniqueName() == "hfastkey-vni" ? HFASTKEY_VNI : HFASTKEY_TELEX;
}
} // namespace

class HfastkeyState final : public InputContextProperty {
public:
    explicit HfastkeyState(InputContext *ic) : ic_(ic), engine_(hfastkey_new(HFASTKEY_TELEX)) {}
    ~HfastkeyState() override { hfastkey_free(engine_); }
    HfastkeyState(const HfastkeyState &) = delete;
    HfastkeyState &operator=(const HfastkeyState &) = delete;

    void keyEvent(const InputMethodEntry &entry, KeyEvent &event) {
        hfastkey_set_method(engine_, methodFor(entry));
        const Key &key = event.rawKey();

        if (event.isRelease()) {
            if (swallowRelease_ != 0 && key.code() == swallowRelease_) {
                swallowRelease_ = 0;
                event.filterAndAccept();
            }
            return;
        }
        if (key.isModifier()) {
            return;
        }

        const bool empty = hfastkey_is_empty(engine_);
        if (key.states().testAny(
                KeyStates{KeyState::Ctrl, KeyState::Alt, KeyState::Super})) {
            if (!empty) {
                commitAndForward(event);
            }
            return;
        }

        if (key.check(FcitxKey_BackSpace)) {
            if (!empty) {
                hfastkey_backspace(engine_);
                updatePreedit();
                event.filterAndAccept();
            }
            return;
        }

        const uint32_t ch = Key::keySymToUnicode(key.sym());
        if (ch >= 0x20 && ch < 0x7f) {
            if (hfastkey_process(engine_, ch)) {
                updatePreedit();
                event.filterAndAccept();
                return;
            }
            if (!empty) {
                std::string text = hfastkey_text(engine_);
                text.push_back(static_cast<char>(ch));
                hfastkey_reset(engine_);
                ic_->commitString(text);
                updatePreedit();
                event.filterAndAccept();
            }
            return;
        }

        // Enter, Tab, Escape, mũi tên, phím chức năng, ký tự ngoài ASCII...
        if (!empty) {
            commitAndForward(event);
        }
    }

    // Chốt từ đang gõ vào ứng dụng.
    void commit() {
        if (hfastkey_is_empty(engine_)) {
            return;
        }
        std::string text = hfastkey_text(engine_);
        hfastkey_reset(engine_);
        ic_->commitString(text);
        updatePreedit();
    }

    // Bỏ từ đang gõ (ứng dụng tự xử lý chữ gạch chân khi mất focus).
    void clear() {
        swallowRelease_ = 0;
        if (!hfastkey_is_empty(engine_)) {
            hfastkey_reset(engine_);
            updatePreedit();
        }
    }

private:
    void commitAndForward(KeyEvent &event) {
        commit();
        ic_->forwardKey(event.rawKey(), false, event.time());
        ic_->forwardKey(event.rawKey(), true, event.time());
        swallowRelease_ = event.rawKey().code();
        event.filterAndAccept();
    }

    void updatePreedit() {
        auto &panel = ic_->inputPanel();
        panel.reset();
        const std::string s = hfastkey_text(engine_);
        if (!s.empty()) {
            const bool client =
                ic_->capabilityFlags().test(CapabilityFlag::Preedit);
            Text text(s, client ? TextFormatFlag::Underline
                                : TextFormatFlag::NoFlag);
            text.setCursor(static_cast<int>(s.size()));
            if (client) {
                panel.setClientPreedit(text);
            } else {
                panel.setPreedit(text);
            }
        }
        ic_->updatePreedit();
        ic_->updateUserInterface(UserInterfaceComponent::InputPanel);
    }

    InputContext *ic_;
    VnEngine *engine_;
    int swallowRelease_ = 0;
};

class HfastkeyEngine final : public InputMethodEngine {
public:
    explicit HfastkeyEngine(Instance *instance)
        : factory_([](InputContext &ic) { return new HfastkeyState(&ic); }) {
        instance->inputContextManager().registerProperty("hfastkeyState",
                                                         &factory_);
    }

    void keyEvent(const InputMethodEntry &entry, KeyEvent &event) override {
        state(event)->keyEvent(entry, event);
    }

    void reset(const InputMethodEntry & /*entry*/,
               InputContextEvent &event) override {
        state(event)->clear();
    }

    void deactivate(const InputMethodEntry & /*entry*/,
                    InputContextEvent &event) override {
        if (event.type() == EventType::InputContextSwitchInputMethod) {
            state(event)->commit();
        }
        state(event)->clear();
    }

private:
    HfastkeyState *state(InputContextEvent &event) {
        return event.inputContext()->propertyFor(&factory_);
    }

    FactoryFor<HfastkeyState> factory_;
};

class HfastkeyFactory final : public AddonFactory {
public:
    AddonInstance *create(AddonManager *manager) override {
        return new HfastkeyEngine(manager->instance());
    }
};

} // namespace fcitx

FCITX_ADDON_FACTORY_V2(hfastkey, fcitx::HfastkeyFactory)
