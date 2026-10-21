/* SPDX-License-Identifier: Apache-2.0 */
/* Place native static user text/rodata/data in separate pages below the image limit. */
OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}
SECTIONS {
    /* Abort-only applications do not carry unwinding metadata or personality references. */
    /DISCARD/ : { *(.eh_frame*) *(.gcc_except_table*) *(.data.DW.ref.rust_eh_personality) *(.note*) *(.comment) }
    . = 0x400000;
    .text : { *(.text .text.*) } :text
    . = ALIGN(0x1000);
    .rodata : { *(.rodata .rodata.*) } :rodata
    . = ALIGN(0x1000);
    .data : { *(.data .data.*) *(.got .got.*) } :data
    .bss : { *(.bss .bss.*) *(COMMON) } :data
}
