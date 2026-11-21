// SPDX-License-Identifier: Apache-2.0
// Check C++ compilation, inherited libstdc++ linking, output and a nonzero guest exit status.
#include <cstdio>
#include <string>

int main() {
    volatile int base = 37;
    const auto output = std::string("stage1-cpp=") + std::to_string(base + 5);
    std::puts(output.c_str());
    return 23;
}
