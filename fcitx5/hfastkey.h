// Giao diện C của lõi Rust (src/ffi.rs).
#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VnEngine VnEngine;

enum { HFASTKEY_TELEX = 0, HFASTKEY_VNI = 1 };

VnEngine *hfastkey_new(uint32_t method);
void hfastkey_free(VnEngine *e);
void hfastkey_set_method(VnEngine *e, uint32_t method);
void hfastkey_set_modern_tone(VnEngine *e, bool modern);
// true nếu phím được đưa vào từ đang gõ.
bool hfastkey_process(VnEngine *e, uint32_t ch);
// true nếu đã xoá một ký tự trong từ đang gõ.
bool hfastkey_backspace(VnEngine *e);
void hfastkey_reset(VnEngine *e);
bool hfastkey_is_empty(VnEngine *e);
// UTF-8, hợp lệ đến lần gọi kế tiếp trên cùng engine.
const char *hfastkey_text(VnEngine *e);

#ifdef __cplusplus
}
#endif
