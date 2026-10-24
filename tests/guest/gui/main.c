/* SPDX-License-Identifier: Apache-2.0 */
/* Independently compile a freestanding C client and exercise real GUI input and lifecycle. */
#include "anova.h"
#include "anova_gui.h"
#include "contract.h"

static uint8_t text[64];
static uint32_t text_length;
static const struct anova_gui_rect field = {12, 84, 216, 24};
static const struct anova_gui_rect button = {12, 116, 100, 28};

static int render(uint64_t window, const struct anova_gui_event *event, uint32_t *activated) {
    struct anova_gui_rect accent = {12, 44, 216, 12};
    static const uint8_t title[] = "ANOVA GUI";
    static const uint8_t subtitle[] = "C / C++ / Rust parity";
    if (anova_gui_window_clear(window, 0x182838) != 0 ||
        anova_gui_draw_rect(window, accent, 0x3070b0) != 0 ||
        anova_gui_draw_text(window, 12, 24, title, sizeof(title) - 1, 0xffffff) != 0 ||
        anova_gui_draw_text(window, 12, 64, subtitle, sizeof(subtitle) - 1, 0xffffff) != 0 ||
        anova_gui_draw_textfield(window, field, text, sizeof(text), &text_length, event) != 0 ||
        anova_gui_draw_button(window, button, (const uint8_t *)"Click", 5, event, activated) != 0)
        return 1;
    int32_t status = anova_gui_window_present(window);
    return status != 0 && status != ANOVA_GUI_WOULD_BLOCK;
}

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    int contract = gui_contract();
    if (contract != 0) return 20 + contract;
    anova_write("[GUI:C:CONTRACT] PASS\n", 22);
    struct anova_gui_window_desc description = {
        ANOVA_GUI_ABI_VERSION, sizeof(description), 80, 80, 240, 160, ANOVA_GUI_VISIBLE, 0
    };
    uint64_t window = 0;
    uint32_t activated = 0, saw_key = 0, saw_pointer = 0;
    if (anova_gui_window_create(&description, &window) != 0 || render(window, 0, &activated))
        return 1;
    anova_write("[GUI:C:READY]\n", 14);
    for (;;) {
        struct anova_gui_event event = {0};
        int32_t status = anova_gui_poll_event(window, &event);
        if (status == ANOVA_GUI_WOULD_BLOCK) { anova_sleep(1); continue; }
        if (status != 0) return 2;
        if (event.kind == ANOVA_GUI_EVENT_KEY && event.state == ANOVA_GUI_KEY_PRESS &&
            event.codepoint == 'q' && saw_key && saw_pointer) break;
        if (render(window, &event, &activated)) return 3;
        if (!saw_key && event.kind == ANOVA_GUI_EVENT_KEY && event.codepoint == 'a' &&
            event.state == ANOVA_GUI_KEY_PRESS && text_length == 1 && text[0] == 'a') {
            saw_key = 1;
            static const char marker[] = "[GUI:C:KEY] codepoint=97 text=a\n";
            anova_write(marker, sizeof(marker) - 1);
        }
        if (!saw_pointer && activated) {
            saw_pointer = 1;
            static const char marker[] = "[GUI:C:POINTER] activated=true\n";
            anova_write(marker, sizeof(marker) - 1);
        }
    }
    if (anova_gui_window_destroy(window) != 0) return 4;
    anova_write("[GUI:C:DONE]\n", 13);
    return 0;
}
