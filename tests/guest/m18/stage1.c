// SPDX-License-Identifier: Apache-2.0
// Check C compilation, static musl linking, output and a nonzero guest exit status.
#include <stdio.h>

int main(void) {
    volatile int base = 37;
    printf("stage1-c=%d\n", base + 5);
    return 17;
}
