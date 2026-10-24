// SPDX-License-Identifier: Apache-2.0
// Verify the inherited guest C++11 seed; this fixture does not reconstruct a compiler.
#include <cstdio>
#include <memory>
#include <vector>

int main() {
    static_assert(__cplusplus >= 201103L, "The GCC seed probe requires C++11");
    std::unique_ptr<int> base(new int(40));
    const std::vector<int> increments{1, 1};
    const auto add = [](int left, int right) { return left + right; };
    int value = *base;
    for (int increment : increments) value = add(value, increment);
    if (value != 42) return 1;
    std::printf("[M15:GCC:SEED] cxx11=ok value=%d\n", value);
    return 0;
}
