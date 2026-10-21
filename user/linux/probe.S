# SPDX-License-Identifier: Apache-2.0
# Pass the Linux initial stack to the freestanding ABI probe with SysV call alignment.
.section .text
.global _start
_start:
    xor %ebp,%ebp
    mov %rsp,%rdi
    and $-16,%rsp
    call probe_main
    ud2
.section .note.GNU-stack,"",@progbits
