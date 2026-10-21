// SPDX-License-Identifier: Apache-2.0
// Intel SDM Vol. 3A, interrupt handling in IA-32e mode. SS:RSP are always saved.
.section .text
.macro ISR number
anova_isr_\number:
    .if (\number != 8) && (\number != 10) && (\number != 11) && (\number != 12) && (\number != 13) && (\number != 14) && (\number != 17) && (\number != 21) && (\number != 29) && (\number != 30)
        push 0
    .endif
    push \number
    jmp anova_isr_common
.endm
.altmacro
.set vector, 0
.rept 256
    ISR %vector
    .set vector, vector + 1
.endr

anova_isr_common:
    push rax
    push rbx
    push rcx
    push rdx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    cld
    mov rdi, rsp
    mov r12, rsp
    and rsp, -16
    call anova_trap_dispatch
    mov rsp, r12
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16
    iretq

.section .rodata
.balign 8
.global anova_isr_table
anova_isr_table:
.macro ISR_ADDRESS number
    .quad anova_isr_\number
.endm
.set vector, 0
.rept 256
    ISR_ADDRESS %vector
    .set vector, vector + 1
.endr
