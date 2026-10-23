/* SPDX-License-Identifier: Apache-2.0 */
/* Native C++ application example: stack RAII descriptor ownership without a libc or STL. */
#include "anova.hpp"

extern "C" int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    anova_info info = {};
    if (anova_get_info(&info) != 0 || info.abi_version != ANOVA_ABI_VERSION ||
        info.size != sizeof(info)) return 1;
    static const char text[] = "[M13:CPP] native SDK v1 RAII\n";
    {
        anova::file console(anova_dup2(1, 15));
        if (!console.valid() || console.write_all(text, sizeof(text) - 1) < 0) return 2;
    }
    /* The destructor must release descriptor 15 before this check. */
    if (anova_write_fd(15, "x", 1) != ANOVA_BAD_DESCRIPTOR) return 3;
    return 0;
}
