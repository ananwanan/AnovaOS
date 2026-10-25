// SPDX-License-Identifier: Apache-2.0
// Deliberately require an absent symbol so a real guest linker must reject the build.
extern int missing_m11_symbol(void);
int main(void) { return missing_m11_symbol(); }
