// SPDX-License-Identifier: Apache-2.0
// Verify guest-built C++ containers, unwinding/RAII, exceptions and a real worker thread.
#include <atomic>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>
struct Guard {
    int &count;
    ~Guard() { ++count; }
};
int main() {
    std::vector<std::string> words = {"AnovaOS", "C++"};
    int destroyed = 0;
    try {
        Guard guard{destroyed};
        throw std::runtime_error(words.at(0));
    } catch (const std::exception &error) {
        if (words.front() != error.what()) return 2;
    }
    std::atomic<int> value{0};
    std::thread worker([&] { value.store(42); });
    worker.join();
    if (destroyed != 1 || value.load() != 42 || words.size() != 2) return 3;
    std::puts("[M11:CPP] string vector RAII exceptions thread");
    return 0;
}
