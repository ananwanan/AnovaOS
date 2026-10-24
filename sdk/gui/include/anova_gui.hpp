/* SPDX-License-Identifier: Apache-2.0 */
/* Own one GUI v1 window using move-only RAII without exceptions, STL or allocation. */
#ifndef ANOVA_GUI_V1_HPP
#define ANOVA_GUI_V1_HPP
#include "anova_gui.h"

namespace anova {
namespace gui {
class window {
    uint64_t handle_;
public:
    window() noexcept : handle_(0) {}
    ~window() { if (handle_) anova_gui_window_destroy(handle_); }
    window(const window &) = delete;
    window &operator=(const window &) = delete;
    window(window &&other) noexcept : handle_(other.handle_) { other.handle_ = 0; }
    window &operator=(window &&other) noexcept {
        if (this != &other) {
            if (handle_) anova_gui_window_destroy(handle_);
            handle_ = other.handle_;
            other.handle_ = 0;
        }
        return *this;
    }
    bool valid() const noexcept { return handle_ != 0; }
    uint64_t native_handle() const noexcept { return handle_; }
    int32_t create(const anova_gui_window_desc &description) noexcept {
        if (valid()) return ANOVA_GUI_BUSY;
        return anova_gui_window_create(&description, &handle_);
    }
    int32_t close() noexcept {
        if (!valid()) return ANOVA_GUI_STALE;
        uint64_t previous = handle_;
        handle_ = 0;
        return anova_gui_window_destroy(previous);
    }
    int32_t clear(uint32_t rgb) const noexcept { return anova_gui_window_clear(handle_, rgb); }
    int32_t rectangle(anova_gui_rect bounds, uint32_t rgb) const noexcept {
        return anova_gui_draw_rect(handle_, bounds, rgb);
    }
    int32_t text(int32_t x, int32_t y, const char *bytes, uint32_t length,
                 uint32_t rgb = 0xffffff) const noexcept {
        return anova_gui_draw_text(handle_, x, y, (const uint8_t *)bytes, length, rgb);
    }
    int32_t button(anova_gui_rect bounds, const char *bytes, uint32_t length,
                   const anova_gui_event *event, uint32_t *activated) const noexcept {
        return anova_gui_draw_button(handle_, bounds, (const uint8_t *)bytes, length,
                                     event, activated);
    }
    int32_t textfield(anova_gui_rect bounds, uint8_t *bytes, uint32_t capacity,
                      uint32_t *length, const anova_gui_event *event) const noexcept {
        return anova_gui_draw_textfield(handle_, bounds, bytes, capacity, length, event);
    }
    int32_t present() const noexcept { return anova_gui_window_present(handle_); }
    int32_t poll(anova_gui_event &event) const noexcept {
        return anova_gui_poll_event(handle_, &event);
    }
};
}
}
#endif
