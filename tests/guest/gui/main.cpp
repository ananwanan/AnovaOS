/* SPDX-License-Identifier: Apache-2.0 */
/* Independently compile a freestanding C++ client with move-only GUI ownership. */
#include "anova.h"
#include "anova_gui.hpp"
#include "contract.h"

static uint8_t text[64];
static uint32_t text_length;
static const anova_gui_rect field = {12, 84, 216, 24};
static const anova_gui_rect button = {12, 116, 100, 28};

static int render(const anova::gui::window &window, const anova_gui_event *event,
                  uint32_t *activated) {
    anova_gui_rect accent = {12, 44, 216, 12};
    if (window.clear(0x182838) != 0 || window.rectangle(accent, 0x3070b0) != 0 ||
        window.text(12, 24, "ANOVA GUI", 9) != 0 ||
        window.text(12, 64, "C / C++ / Rust parity", 21) != 0 ||
        window.textfield(field, text, sizeof(text), &text_length, event) != 0 ||
        window.button(button, "Click", 5, event, activated) != 0) return 1;
    int32_t status = window.present();
    return status != 0 && status != ANOVA_GUI_WOULD_BLOCK;
}

extern "C" int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    int contract = gui_contract();
    if (contract != 0) return 20 + contract;
    static const char checked[] = "[GUI:CPP:CONTRACT] PASS\n";
    anova_write(checked, sizeof(checked) - 1);
    anova_gui_window_desc description = {
        ANOVA_GUI_ABI_VERSION, sizeof(description), 80, 80, 240, 160, ANOVA_GUI_VISIBLE, 0
    };
    uint64_t destructed = 0;
    {
        anova_gui_window_desc hidden = description;
        hidden.flags = 0;
        anova::gui::window scoped;
        if (scoped.create(hidden) != 0) return 8;
        uint64_t assigned = scoped.native_handle();
        anova::gui::window empty;
        scoped = static_cast<anova::gui::window &&>(empty);
        if (scoped.valid() || anova_gui_window_present(assigned) != ANOVA_GUI_STALE)
            return 9;
        if (scoped.create(hidden) != 0) return 10;
        destructed = scoped.native_handle();
    }
    if (anova_gui_window_present(destructed) != ANOVA_GUI_STALE) return 11;
    anova::gui::window first;
    if (first.create(description) != 0) return 1;
    anova::gui::window moved(static_cast<anova::gui::window &&>(first));
    if (first.valid() || first.close() != ANOVA_GUI_STALE || !moved.valid()) return 2;
    anova::gui::window window;
    window = static_cast<anova::gui::window &&>(moved);
    if (moved.valid() || !window.valid()) return 3;
    uint32_t activated = 0;
    bool saw_key = false, saw_pointer = false;
    if (render(window, nullptr, &activated)) return 4;
    anova_write("[GUI:CPP:READY]\n", 16);
    for (;;) {
        anova_gui_event event = {};
        int32_t status = window.poll(event);
        if (status == ANOVA_GUI_WOULD_BLOCK) { anova_sleep(1); continue; }
        if (status != 0) return 5;
        if (event.kind == ANOVA_GUI_EVENT_KEY && event.state == ANOVA_GUI_KEY_PRESS &&
            event.codepoint == 'q' && saw_key && saw_pointer) break;
        if (render(window, &event, &activated)) return 6;
        if (!saw_key && event.kind == ANOVA_GUI_EVENT_KEY && event.codepoint == 'a' &&
            event.state == ANOVA_GUI_KEY_PRESS && text_length == 1 && text[0] == 'a') {
            saw_key = true;
            static const char marker[] = "[GUI:CPP:KEY] codepoint=97 text=a\n";
            anova_write(marker, sizeof(marker) - 1);
        }
        if (!saw_pointer && activated) {
            saw_pointer = true;
            static const char marker[] = "[GUI:CPP:POINTER] activated=true\n";
            anova_write(marker, sizeof(marker) - 1);
        }
    }
    uint64_t closed = window.native_handle();
    if (window.close() != 0 || window.close() != ANOVA_GUI_STALE ||
        anova_gui_window_present(closed) != ANOVA_GUI_STALE) return 7;
    anova_write("[GUI:CPP:DONE]\n", 15);
    return 0;
}
