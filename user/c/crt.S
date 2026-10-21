/* SPDX-License-Identifier: Apache-2.0 */
/* Convert the native startup stack to C main arguments and exit through SYSCALL. */
.section .text
.global _start
.type _start,@function
_start:
    xor %ebp,%ebp
    mov (%rsp),%rdi
    lea 8(%rsp),%rsi
    and $-16,%rsp
    call main
    mov %eax,%edi
    mov $1,%eax
    syscall
    ud2
.section .note.GNU-stack,"",@progbits
