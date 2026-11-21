// SPDX-License-Identifier: Apache-2.0
// Exercise the actual rebuilt LLVM demangler through four mangling schemes and invalid input.

#include "llvm/Demangle/Demangle.h"
#include <cstdio>
#include <cstdlib>

int main()
{
    if (llvm::demangle("_Z3foov") != "foo()" ||
        llvm::demangle("?func@@YAHH@Z") != "int __cdecl func(int)" ||
        llvm::demangle("_RNvC3foo3bar") != "foo::bar" ||
        llvm::demangle("_D8demangle4testZ") != "demangle.test") {
        std::fputs("[M18:LLVM:FAIL] demangled name differs\n", stderr);
        return 1;
    }
    char *invalid = llvm::itaniumDemangle("not-a-symbol");
    if (invalid) {
        std::free(invalid);
        std::fputs("[M18:LLVM:FAIL] invalid Itanium input accepted\n", stderr);
        return 1;
    }
    std::puts("[M18:LLVM:DEMANGLE] itanium=ok microsoft=ok rust-v0=ok d=ok invalid=rejected");
    return 0;
}
