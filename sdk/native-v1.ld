/* SPDX-License-Identifier: Apache-2.0 */
/* Link native SDK v1 user images with separate W^X pages and nonempty valid LOAD segments. */
OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}
SECTIONS {
    /DISCARD/ : { *(.eh_frame*) *(.gcc_except_table*) *(.data.DW.ref.rust_eh_personality) *(.note*) *(.comment) }
    . = 0x400000;
    .text : { *(.text .text.*) } :text
    . = ALIGN(0x1000);
    /* Keep metadata-only programs valid under both GNU ld and rust-lld. */
    .rodata : { BYTE(0); *(.rodata .rodata.*) } :rodata
    . = ALIGN(0x1000);
    .data : { BYTE(0); *(.data .data.*) *(.got .got.*) } :data
    .bss : { *(.bss .bss.*) *(COMMON) } :data
}
