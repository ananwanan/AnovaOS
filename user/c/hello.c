/* SPDX-License-Identifier: Apache-2.0 */
/* Show the C SDK, arguments, BSS/SSE2 and scheduling/descriptor calls before exit 7. */
#include "anova.h"
static volatile uint64_t bss;
static size_t length(const char *text) { size_t n = 0; while (text[n]) ++n; return n; }
int main(int argc, char **argv) {
    struct anova_info info = {0};
    uint64_t simd = 0;
    uint16_t cs = 0;
    __asm__ volatile ("mov %%cs,%0" : "=r"(cs));
    __asm__ volatile ("pxor %%xmm0,%%xmm0; pcmpeqd %%xmm1,%%xmm1; paddq %%xmm1,%%xmm0; movq %%xmm0,%0"
        : "=r"(simd) : : "xmm0", "xmm1");
    if ((cs & 3) != 3 || bss != 0 || simd != UINT64_MAX ||
        anova_get_info(&info) != 0 || info.abi_version != ANOVA_ABI_VERSION) return 101;
    bss = 42;
    if (anova_yield() != 0 || anova_sleep(0) != 0 || anova_close(2) != 0 || anova_close(2) != -9)
        return 101;
    const char *prefix = "[APP:C] hello from Ring 3; args:";
    anova_write(prefix, length(prefix));
    for (int i = 0; i < argc; ++i) { anova_write(" ", 1); anova_write(argv[i], length(argv[i])); }
    const char *suffix = "; BSS=zero SSE2=ok\n";
    anova_write(suffix, length(suffix));
    return 7;
}
