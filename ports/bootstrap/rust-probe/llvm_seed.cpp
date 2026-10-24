// SPDX-License-Identifier: Apache-2.0
// Check the inherited C++17 seed language and library required by the audited LLVM release.
#include <cstdio>
#include <optional>

template <typename... Values> constexpr int sum(Values... values) {
    return (values + ...);
}

int main() {
    static_assert(__cplusplus >= 201703L, "The LLVM seed probe requires C++17");
    static_assert(sum(19, 23) == 42, "The LLVM seed probe requires fold expressions");
    const std::optional<int> value(sum(19, 23));
    if (!value || *value != 42) return 1;
    std::printf("[M15:LLVM:SEED] cxx17=ok value=%d\n", *value);
    return 0;
}
