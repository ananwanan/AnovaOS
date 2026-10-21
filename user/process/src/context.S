// SPDX-License-Identifier: Apache-2.0
// Keep distinct GPR, all XMM lanes, x87 stack/control and MXCSR sentinels live
// across syscall-free timer preemption and optional eager fork, then verify them.
.section .text
.global anova_context_burn
anova_context_burn:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    sub rsp,40
    mov [rsp],rdi
    mov qword ptr [rsp+8],0
    mov [rsp+32],rdx
    mov eax,edi
    and eax,3
    shl eax,10
    or eax,0x37f
    mov [rsp+16],ax
    fldcw [rsp+16]
    mov eax,edi
    and eax,3
    shl eax,13
    or eax,0x1f80
    mov [rsp+20],eax
    ldmxcsr [rsp+20]
    fild qword ptr [rsp]
.macro FILL reg
    movq \reg,rdi
    punpcklqdq \reg,\reg
.endm
    FILL xmm0
    FILL xmm1
    FILL xmm2
    FILL xmm3
    FILL xmm4
    FILL xmm5
    FILL xmm6
    FILL xmm7
    FILL xmm8
    FILL xmm9
    FILL xmm10
    FILL xmm11
    FILL xmm12
    FILL xmm13
    FILL xmm14
    FILL xmm15
    mov rbx,rdi
    mov rbp,rdi
    mov r8,rdi
    mov r9,rdi
    mov r10,rdi
    mov r12,rdi
    mov r13,rdi
    mov r14,rdi
    mov r15,rdi
    cmp qword ptr [rsp+32],0
    je 1f
    mov eax,8
    syscall
    mov [rsp+8],rax
1:
    mov r11,rdi
    mov rcx,rsi
2:
    dec rcx
    jnz 2b
.macro CHECK_GPR reg
    cmp \reg,[rsp]
    jne 9f
.endm
    CHECK_GPR rdi
    CHECK_GPR rbx
    CHECK_GPR rbp
    CHECK_GPR r8
    CHECK_GPR r9
    CHECK_GPR r10
    CHECK_GPR r11
    CHECK_GPR r12
    CHECK_GPR r13
    CHECK_GPR r14
    CHECK_GPR r15
.macro CHECK_XMM reg
    movq rax,\reg
    cmp rax,[rsp]
    jne 9f
    psrldq \reg,8
    movq rax,\reg
    cmp rax,[rsp]
    jne 9f
.endm
    CHECK_XMM xmm0
    CHECK_XMM xmm1
    CHECK_XMM xmm2
    CHECK_XMM xmm3
    CHECK_XMM xmm4
    CHECK_XMM xmm5
    CHECK_XMM xmm6
    CHECK_XMM xmm7
    CHECK_XMM xmm8
    CHECK_XMM xmm9
    CHECK_XMM xmm10
    CHECK_XMM xmm11
    CHECK_XMM xmm12
    CHECK_XMM xmm13
    CHECK_XMM xmm14
    CHECK_XMM xmm15
    fnstcw [rsp+24]
    mov ax,[rsp+24]
    cmp ax,[rsp+16]
    jne 9f
    stmxcsr [rsp+24]
    mov eax,[rsp+24]
    cmp eax,[rsp+20]
    jne 9f
    fistp qword ptr [rsp+24]
    mov rax,[rsp+24]
    cmp rax,[rsp]
    jne 9f
    mov rax,[rsp+8]
    jmp 10f
9:
    mov rax,-100
10:
    fninit
    mov dword ptr [rsp+20],0x1f80
    ldmxcsr [rsp+20]
    add rsp,40
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
.section .note.GNU-stack,"",@progbits
