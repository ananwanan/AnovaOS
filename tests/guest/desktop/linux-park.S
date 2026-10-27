/* SPDX-License-Identifier: Apache-2.0 */
/* Run one real Linux nanosleep/exit so native child supervision must reject a live Linux image. */
.section .text
.global _start
_start:
    mov $35, %rax
    lea interval(%rip), %rdi
    xor %rsi, %rsi
    syscall
    test %rax, %rax
    jne failed
    xor %rdi, %rdi
    jmp exit
failed:
    mov $1, %rdi
exit:
    mov $60, %rax
    syscall
    ud2
.section .rodata
.balign 8
interval:
    .quad 0, 200000000
.section .note.GNU-stack,"",@progbits
