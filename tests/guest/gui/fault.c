/* SPDX-License-Identifier: Apache-2.0 */
/* Deliberately violate the GUI output-pointer contract to verify isolation of the entire client. */
#include "anova.h"
#include "anova_gui.h"

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    struct anova_gui_window_desc description = {
        ANOVA_GUI_ABI_VERSION, sizeof(description), 80, 80, 240, 160, ANOVA_GUI_VISIBLE, 0
    };
    uint64_t window = 0;
    if (anova_gui_window_create(&description, &window) != 0 ||
        anova_gui_window_clear(window, 0x182838) != 0 || anova_gui_window_present(window) != 0)
        return 1;
    static const char ready[] = "[GUI:FAULT:READY]\n";
    static const char inject[] = "[GUI:FAULT:INJECT]\n";
    anova_write(ready, sizeof(ready) - 1);
    anova_write(inject, sizeof(inject) - 1);
    /* Isolation test only: 0x1000 is aligned but unmapped, outside native user memory.
     * The caller intentionally violates the documented valid writable-output invariant. */
    anova_gui_query_version(ANOVA_GUI_ABI_VERSION,
                           (struct anova_gui_version *)(uintptr_t)0x1000);
    return 2;
}
