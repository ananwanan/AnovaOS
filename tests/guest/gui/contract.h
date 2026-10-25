/* SPDX-License-Identifier: Apache-2.0 */
/* Exercise GUI ABI failure contracts in each independently compiled C/C++ client. */
#ifndef ANOVA_GUI_GUEST_CONTRACT_H
#define ANOVA_GUI_GUEST_CONTRACT_H
#include "anova_gui.h"

static int gui_contract(void) {
    struct anova_gui_version version = {0, 0, 0, 0, 0, 0, 0, 0};
    if (anova_gui_query_version(ANOVA_GUI_ABI_VERSION + 1, &version) != ANOVA_GUI_VERSION)
        return 1;
    if (anova_gui_query_version(ANOVA_GUI_ABI_VERSION, &version) != 0 ||
        version.abi_version != ANOVA_GUI_ABI_VERSION || version.struct_size != sizeof(version) ||
        version.event_size != sizeof(struct anova_gui_event)) return 2;
    struct anova_gui_window_desc description = {
        ANOVA_GUI_ABI_VERSION, sizeof(description), 80, 80, 240, 160, 0, 0
    };
    uint64_t handle = 0;
    description.struct_size--;
    if (anova_gui_window_create(&description, &handle) != ANOVA_GUI_INVALID) return 3;
    description.struct_size++;
    description.width = 321;
    if (anova_gui_window_create(&description, &handle) != ANOVA_GUI_INVALID) return 18;
    description.width = 240;
    if (anova_gui_window_create(&description, &handle) != 0 || !handle) return 4;
    uint64_t second = 0;
    if (anova_gui_window_create(&description, &second) != ANOVA_GUI_LIMIT) return 5;
    const uint8_t invalid[] = {0xc0, 0xaf};
    if (anova_gui_draw_text(handle, 12, 24, invalid, 2, 0xffffff) != ANOVA_GUI_UTF8)
        return 6;
    static const uint8_t oversized[1025] = {0};
    if (anova_gui_draw_text(handle, 12, 24, oversized, sizeof(oversized), 0xffffff) != ANOVA_GUI_LIMIT)
        return 7;
    if (anova_gui_window_clear(handle, 0xff000000) != ANOVA_GUI_INVALID) return 8;
    struct anova_gui_rect bounds = {12, 84, 216, 24};
    uint8_t text[2] = {'a', 0};
    uint32_t length = 3;
    if (anova_gui_draw_textfield(handle, bounds, text, 2, &length, 0) != ANOVA_GUI_BUFFER)
        return 9;
    struct anova_gui_layout layout = {{12, 24, 216, 48}, 0, 4, 24};
    struct anova_gui_rect row = {0, 0, 0, 0};
    if (anova_gui_layout_next(&layout, &row) != 0 || row.x != 12 || row.y != 24 ||
        row.width != 216 || row.height != 24) return 10;
    if (anova_gui_layout_next(&layout, &row) != ANOVA_GUI_LIMIT) return 11;
    static const uint8_t clipboard[] = {'c', 'l', 'i', 'p', 0xe2, 0x98, 0x83};
    uint8_t copied[8] = {0};
    uint32_t written = 0;
    if (anova_gui_clipboard_set(clipboard, sizeof(clipboard)) != 0 ||
        anova_gui_clipboard_get(copied, 1, &written) != ANOVA_GUI_BUFFER ||
        anova_gui_clipboard_get(copied, sizeof(copied), &written) != 0 ||
        written != sizeof(clipboard)) return 12;
    for (uint32_t i = 0; i < written; ++i) if (copied[i] != clipboard[i]) return 13;
    if (anova_gui_clipboard_set(invalid, 2) != ANOVA_GUI_UTF8) return 14;
    if (anova_gui_window_destroy(handle) != 0 ||
        anova_gui_window_destroy(handle) != ANOVA_GUI_STALE ||
        anova_gui_window_present(handle) != ANOVA_GUI_STALE) return 15;
    if (anova_gui_window_create(&description, &second) != 0 || second == handle) return 16;
    if (anova_gui_window_clear(handle, 0) != ANOVA_GUI_STALE ||
        anova_gui_window_destroy(second) != 0) return 17;
    return 0;
}
#endif
