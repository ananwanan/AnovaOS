// SPDX-License-Identifier: Apache-2.0
// Enter one Ring 3 context, switch SYSCALL to its trusted stack, and return to
// the saved kernel continuation after exit/fault. BSP only; no GS-based state.
.section .bss
.balign 8
anova_user_saved_rsp: .quad 0
anova_user_saved_cr3: .quad 0
anova_user_kernel_top: .quad 0
anova_user_rsp: .quad 0

.section .text
.global anova_run_user
anova_run_user:
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    mov [rip + anova_user_saved_rsp], rsp
    mov rax, cr3
    mov [rip + anova_user_saved_cr3], rax
    mov [rip + anova_user_kernel_top], rcx
    mov cr3, rdi
    push 0x2b
    push rdx
    push 0x202
    push 0x33
    push rsi
    mov ax, 0x2b
    mov ds, ax
    mov es, ax
    xor eax, eax
    xor ebx, ebx
    xor ecx, ecx
    xor edx, edx
    xor ebp, ebp
    xor edi, edi
    xor esi, esi
    xor r8d, r8d
    xor r9d, r9d
    xor r10d, r10d
    xor r11d, r11d
    xor r12d, r12d
    xor r13d, r13d
    xor r14d, r14d
    xor r15d, r15d
    iretq

.global anova_syscall_entry
anova_syscall_entry:
    // FMASK already cleared IF/TF/DF/AC/NT. No use of the user stack precedes
    // this switch; scratch addresses are permanently supervisor-only.
    mov [rip + anova_user_rsp], rsp
    mov rsp, [rip + anova_user_kernel_top]
    push 0x2b
    push qword ptr [rip + anova_user_rsp]
    push r11
    push 0x33
    push rcx
    push 0
    push 0x80
    jmp anova_isr_common

.global anova_leave_user
anova_leave_user:
    cli
    mov rax, [rip + anova_user_saved_cr3]
    mov cr3, rax
    mov rsp, [rip + anova_user_saved_rsp]
    mov ax, 16
    mov ds, ax
    mov es, ax
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    ret

// The only recoverable kernel memory instruction. #PF fixes RIP to the failure
// label only while a bounded usercopy guard identifies this precise operation.
.global anova_copy_user
anova_copy_user:
    mov rcx, rdx
    xor eax, eax
.global anova_copy_user_fault
anova_copy_user_fault:
    rep movsb
    mov eax, 1
    ret
.global anova_copy_user_failed
anova_copy_user_failed:
    xor eax, eax
    ret
