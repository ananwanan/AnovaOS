/* SPDX-License-Identifier: Apache-2.0 */
/* Run the raw GUI ABI negatives independently from the interactive language demos. */
#include "anova.h"
#include "contract.h"

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    int status = gui_contract();
    if (status != 0) return status;
    static const char marker[] = "[GUI:CONTRACT:DONE]\n";
    anova_write(marker, sizeof(marker) - 1);
    return 0;
}
