/* SPDX-License-Identifier: Apache-2.0 */
/* Native SDK v1 C++ descriptor ownership; no STL, exceptions, allocation or Linux calls. */
#ifndef ANOVA_SDK_V1_HPP
#define ANOVA_SDK_V1_HPP
#include "anova.h"

namespace anova {
/* Raw results keep the C API's stable negative error codes. */
class file {
    int64_t descriptor_;
public:
    explicit file(int64_t descriptor) : descriptor_(descriptor) {}
    ~file() { if (descriptor_ >= 0) anova_close((uint64_t)descriptor_); }
    file(const file &) = delete;
    file &operator=(const file &) = delete;
    file(file &&other) noexcept : descriptor_(other.descriptor_) { other.descriptor_ = -1; }
    file &operator=(file &&other) noexcept {
        if (this != &other) {
            if (descriptor_ >= 0) anova_close((uint64_t)descriptor_);
            descriptor_ = other.descriptor_;
            other.descriptor_ = -1;
        }
        return *this;
    }
    bool valid() const { return descriptor_ >= 0; }
    int64_t native_handle() const { return descriptor_; }
    int64_t write_all(const void *bytes, size_t length) {
        return valid() ? anova_write_all((uint64_t)descriptor_, bytes, length) :
            ANOVA_BAD_DESCRIPTOR;
    }
    int64_t read(void *bytes, size_t length) {
        return valid() ? anova_read((uint64_t)descriptor_, bytes, length) :
            ANOVA_BAD_DESCRIPTOR;
    }
    int64_t sync() const {
        return valid() ? anova_fsync((uint64_t)descriptor_) : ANOVA_BAD_DESCRIPTOR;
    }
    int64_t close() {
        if (!valid()) return ANOVA_BAD_DESCRIPTOR;
        int64_t descriptor = descriptor_;
        descriptor_ = -1;
        return anova_close((uint64_t)descriptor);
    }
};
}
#endif
