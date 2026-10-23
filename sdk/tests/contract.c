/* SPDX-License-Identifier: Apache-2.0 */
/* Host-executed C/C++ checks of frozen SDK errors and layouts without issuing native syscalls. */
#ifdef __cplusplus
#include "anova.hpp"
#else
#include "anova.h"
#endif

static int equal(const char *left, const char *right) {
    while (*left && *left == *right) { ++left; ++right; }
    return *left == *right;
}

int main(void) {
    const int64_t values[] = {
        ANOVA_BAD_ADDRESS, ANOVA_UNSUPPORTED, ANOVA_INVALID_ARGUMENT, ANOVA_TOO_LONG,
        ANOVA_NO_CHILD, ANOVA_NO_MEMORY, ANOVA_NOT_FOUND, ANOVA_PROCESS_LIMIT,
        ANOVA_BAD_DESCRIPTOR, ANOVA_FILE_LIMIT, ANOVA_READ_ONLY, ANOVA_NOT_DIRECTORY,
        ANOVA_IS_DIRECTORY, ANOVA_ALREADY_EXISTS, ANOVA_WOULD_BLOCK, ANOVA_BROKEN_PIPE,
        ANOVA_NOT_SEEKABLE, ANOVA_NO_SPACE, ANOVA_LINK_LOOP, ANOVA_PERMISSION_DENIED,
        ANOVA_IO_ERROR, ANOVA_INVALID_FILESYSTEM, ANOVA_DEVICE_TIMEOUT, ANOVA_BUSY,
        ANOVA_NOT_EMPTY
    };
    for (size_t index = 0; index != sizeof(values) / sizeof(values[0]); ++index) {
        if (values[index] != -(int64_t)(index + 1)) return 1;
        if (equal(anova_error_name(values[index]), "unknown_native_error")) return 2;
    }
    if (!equal(anova_error_name(INT64_MIN), "unknown_native_error")) return 3;
    if (!equal(anova_error_name(-26), "unknown_native_error")) return 4;
    if (!equal(anova_error_name(0), "success")) return 5;
    if (!equal(anova_error_name(17), "success")) return 6;
    if (!equal(anova_error_name(ANOVA_NOT_FOUND), "not_found")) return 7;
    return ANOVA_ABI_VERSION != 1 || ANOVA_MAX_ARGS != 64 ||
        ANOVA_MAX_ARGUMENT_BYTES != 1023 || sizeof(struct anova_info) != 32 ||
        sizeof(struct anova_exec_request) != 32 || sizeof(struct anova_wait_status) != 16 ||
        sizeof(struct anova_dir_entry) != 80;
}
