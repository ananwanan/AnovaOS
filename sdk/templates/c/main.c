/* SPDX-License-Identifier: Apache-2.0 */
/* Native C application example: negotiate v1 and diagnose a real missing-file error. */
#include "anova.h"

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    struct anova_info info = {0};
    if (anova_get_info(&info) != 0 || info.abi_version != ANOVA_ABI_VERSION ||
        info.size != sizeof(info)) return 1;
    static const char hello[] = "[M13:C] native SDK v1\n";
    if (anova_write_all(1, hello, sizeof(hello) - 1) < 0) return 2;
    int64_t missing = anova_open("/m13-missing", 12, ANOVA_OPEN_READ);
    if (missing != ANOVA_NOT_FOUND) return 3;
    const char *name = anova_error_name(missing);
    size_t length = 0;
    while (name[length]) ++length;
    anova_write_all(1, name, length);
    anova_write_all(1, "\n", 1);
    return 0;
}
