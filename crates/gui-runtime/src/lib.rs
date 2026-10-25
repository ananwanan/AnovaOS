//! Provide the bounded native GUI 1.0 C ABI and an ownership-safe Rust window wrapper.
//!
//! One process-local window and clipboard use fixed storage without malloc. Applications
//! serialize calls on one UI thread and must not fork after creating a window. Calls have
//! no callbacks and cannot unwind; overlapping/reentrant state access returns Busy. Input
//! is US ASCII set-1 text conversion; UTF-8 storage is lossless but unsupported glyphs use
//! an original replacement bitmap. Widget event targeting remains application policy.
//!
//! C pointer arguments must reference mapped live storage for the complete synchronous
//! call. Null, alignment, integer span and bounded lengths are checked; this user library
//! cannot validate arbitrary mappings. Violations can fault only the calling process.
//! No pointer or Rust reference is retained after return. Window handles are local
//! generation numbers, never compositor authority or cross-process capabilities.
//!
//! SPDX-License-Identifier: Apache-2.0

#![no_std]

use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

mod render;
mod runtime;
// Reuse the internal, explicit little-endian codec without linking compositor policy.
#[allow(dead_code)]
#[path = "../../gui/src/wire.rs"]
mod wire;

pub const ABI_VERSION: u32 = 0x0001_0000;
pub const MAX_WIDTH: u32 = 320;
pub const MAX_HEIGHT: u32 = 240;
pub const MAX_TEXT_BYTES: u32 = 1024;
pub const CAP_RASTER: u32 = 1;
pub const CAP_WIDGETS: u32 = 2;
pub const CAP_CLIPBOARD: u32 = 4;

/// Every public operation returns zero or one of these stable, negative v1 statuses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum Error {
    Invalid = -1,
    Version = -2,
    Stale = -3,
    Limit = -4,
    Utf8 = -5,
    WouldBlock = -6,
    Disconnected = -7,
    Busy = -8,
    Transport = -9,
    Buffer = -10,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct GuiVersion {
    pub abi_version: u32,
    pub struct_size: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub max_text_bytes: u32,
    pub event_size: u32,
    pub capabilities: u32,
    pub reserved: u32,
}

/// Short alias for the link anchor; the generated C ABI uses GuiVersion's layout.
pub type Version = GuiVersion;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct GuiWindowDesc {
    pub abi_version: u32,
    pub struct_size: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub flags: u32,
    pub reserved: u32,
}

impl GuiWindowDesc {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            abi_version: ABI_VERSION,
            struct_size: 32,
            x,
            y,
            width,
            height,
            flags: 1,
            reserved: 0,
        }
    }

    fn validate(self) -> Result<(), Error> {
        if self.abi_version != ABI_VERSION {
            return Err(Error::Version);
        }
        if self.struct_size != 32 || self.flags & !1 != 0 || self.reserved != 0 {
            return Err(Error::Invalid);
        }
        if self.width < 32 || self.height < 24 || self.width > MAX_WIDTH || self.height > MAX_HEIGHT
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct GuiRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct GuiEvent {
    pub kind: u32,
    pub key: u32,
    pub codepoint: u32,
    pub modifiers: u32,
    pub x: i32,
    pub y: i32,
    pub buttons: u32,
    pub changed: u32,
    pub width: u32,
    pub height: u32,
    pub state: u32,
    pub reserved: u32,
}

impl GuiEvent {
    pub const EMPTY: Self = Self {
        kind: 0,
        key: 0,
        codepoint: 0,
        modifiers: 0,
        x: 0,
        y: 0,
        buttons: 0,
        changed: 0,
        width: 0,
        height: 0,
        state: 0,
        reserved: 0,
    };

    fn validate(self) -> Result<(), Error> {
        if !(1..=3).contains(&self.kind)
            || self.reserved != 0
            || self.modifiers & !15 != 0
            || self.buttons & !7 != 0
            || self.changed & !7 != 0
            || self.key > 383
            || self.state > 3
            || (self.codepoint != 0 && char::from_u32(self.codepoint).is_none())
            || (self.kind == 2 && self.state == 0)
        {
            return Err(Error::Invalid);
        }
        match self.kind {
            1 if self
                != (Self {
                    kind: 1,
                    ..Self::EMPTY
                }) =>
            {
                return Err(Error::Invalid);
            }
            2 if self.x != 0
                || self.y != 0
                || self.buttons != 0
                || self.changed != 0
                || self.width != 0
                || self.height != 0
                || (self.state == 2 && self.codepoint != 0) =>
            {
                return Err(Error::Invalid);
            }
            3 if self.key != 0
                || self.codepoint != 0
                || self.modifiers != 0
                || self.state != 0
                || (self.width == 0) != (self.height == 0)
                || self.width > MAX_WIDTH
                || self.height > MAX_HEIGHT =>
            {
                return Err(Error::Invalid);
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct GuiLayout {
    pub bounds: GuiRect,
    pub cursor: u32,
    pub gap: u32,
    pub row_height: u32,
}

impl GuiLayout {
    /// Allocate one full-width row without modifying the cursor on exhaustion.
    pub fn next_row(&mut self) -> Result<GuiRect, Error> {
        if self.bounds.width == 0 || self.row_height == 0 {
            return Err(Error::Invalid);
        }
        let end = self
            .cursor
            .checked_add(self.row_height)
            .ok_or(Error::Limit)?;
        if end > self.bounds.height {
            return Err(Error::Limit);
        }
        let y = i64::from(self.bounds.y) + i64::from(self.cursor);
        let y = i32::try_from(y).map_err(|_| Error::Invalid)?;
        let cursor = end.checked_add(self.gap).ok_or(Error::Limit)?;
        let rect = GuiRect {
            x: self.bounds.x,
            y,
            width: self.bounds.width,
            height: self.row_height,
        };
        self.cursor = cursor;
        Ok(rect)
    }
}

struct Global {
    busy: AtomicBool,
    value: UnsafeCell<runtime::Runtime<runtime::Native>>,
}
// SAFETY: The acquire/release busy guard admits exactly one mutable access to the
// UnsafeCell. No state reference escapes the guarded closure, including on error.
unsafe impl Sync for Global {}

static GLOBAL: Global = Global {
    busy: AtomicBool::new(false),
    value: UnsafeCell::new(runtime::Runtime::new(runtime::Native)),
};

struct BusyGuard;
impl Drop for BusyGuard {
    fn drop(&mut self) {
        GLOBAL.busy.store(false, Ordering::Release);
    }
}

fn state(
    operation: impl FnOnce(&mut runtime::Runtime<runtime::Native>) -> Result<(), Error>,
) -> i32 {
    if GLOBAL
        .busy
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return Error::Busy as i32;
    }
    let _guard = BusyGuard;
    // SAFETY: The successful atomic exchange establishes exclusive access until
    // _guard drops; the closure cannot retain a borrowed state reference.
    status(operation(unsafe { &mut *GLOBAL.value.get() }))
}

fn status(result: Result<(), Error>) -> i32 {
    result.map_or_else(|error| error as i32, |()| 0)
}

fn pointer<T>(value: *const T, count: usize) -> Result<(), Error> {
    let bytes = size_of::<T>().checked_mul(count).ok_or(Error::Invalid)?;
    if value.is_null()
        || !(value as usize).is_multiple_of(align_of::<T>())
        || (value as usize)
            .checked_add(bytes)
            .is_none_or(|end| end > isize::MAX as usize)
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

unsafe fn text<'a>(value: *const u8, length: u32) -> Result<&'a str, Error> {
    if length > MAX_TEXT_BYTES {
        return Err(Error::Limit);
    }
    if length == 0 {
        return Ok("");
    }
    pointer(value, length as usize)?;
    // SAFETY: The FFI caller guarantees this checked span remains readable and
    // immutable for the call; length is bounded before constructing the slice.
    core::str::from_utf8(unsafe { core::slice::from_raw_parts(value, length as usize) })
        .map_err(|_| Error::Utf8)
}

unsafe fn optional_event(value: *const GuiEvent) -> Result<Option<GuiEvent>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    pointer(value, 1)?;
    // SAFETY: The caller supplies a live aligned integer-only event. Copying it
    // before borrowing any text permits application-owned input storage reuse.
    let event = unsafe { value.read() };
    event.validate()?;
    Ok(Some(event))
}

fn color(value: u32) -> Result<(), Error> {
    if value & 0xff00_0000 == 0 {
        Ok(())
    } else {
        Err(Error::Invalid)
    }
}

/// Negotiate the exact frozen v1 ABI before opening a connection.
/// # Safety
/// `output` is an exclusively writable live GuiVersion for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_query_version(requested: u32, output: *mut GuiVersion) -> i32 {
    status((|| {
        if requested != ABI_VERSION {
            return Err(Error::Version);
        }
        pointer(output, 1)?;
        // SAFETY: The caller owns the checked aligned output; every field is initialized.
        unsafe {
            output.write(GuiVersion {
                abi_version: ABI_VERSION,
                struct_size: 32,
                max_width: MAX_WIDTH,
                max_height: MAX_HEIGHT,
                max_text_bytes: MAX_TEXT_BYTES,
                event_size: 48,
                capabilities: CAP_RASTER | CAP_WIDGETS | CAP_CLIPBOARD,
                reserved: 0,
            });
        }
        Ok(())
    })())
}

/// Connect and create one process-local window; out remains untouched on error.
/// # Safety
/// `desc` is readable and `output` exclusively writable live aligned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_window_create(
    desc: *const GuiWindowDesc,
    output: *mut u64,
) -> i32 {
    if let Err(error) = pointer(desc, 1).and_then(|()| pointer(output, 1)) {
        return error as i32;
    }
    // SAFETY: The caller provides a live integer-only descriptor; copy before state access.
    let desc = unsafe { desc.read() };
    state(|runtime| {
        let handle = runtime.create(desc)?;
        // SAFETY: The caller retains exclusive ownership of this checked output.
        unsafe {
            output.write(handle);
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn anova_gui_window_destroy(handle: u64) -> i32 {
    state(|runtime| runtime.destroy(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn anova_gui_window_show(handle: u64, visible: u32) -> i32 {
    if visible > 1 {
        return Error::Invalid as i32;
    }
    state(|runtime| runtime.show(handle, visible != 0))
}

#[unsafe(no_mangle)]
pub extern "C" fn anova_gui_window_clear(handle: u64, rgb: u32) -> i32 {
    state(|runtime| {
        color(rgb)?;
        runtime.canvas(handle)?.pixels.fill(rgb);
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn anova_gui_draw_rect(handle: u64, rect: GuiRect, rgb: u32) -> i32 {
    state(|runtime| {
        color(rgb)?;
        runtime.canvas(handle)?.rect(rect, rgb);
        Ok(())
    })
}

/// Render length-delimited UTF-8; no NUL terminator is required or retained.
/// # Safety
/// `bytes` is readable for length bytes, unless length is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_draw_text(
    handle: u64,
    x: i32,
    y: i32,
    bytes: *const u8,
    length: u32,
    rgb: u32,
) -> i32 {
    // SAFETY: The caller supplies readable text for this call; text checks bounds/UTF-8.
    let text = match unsafe { text(bytes, length) } {
        Ok(value) => value,
        Err(error) => return error as i32,
    };
    state(|runtime| {
        color(rgb)?;
        runtime.canvas(handle)?.text(x, y, text, rgb);
        Ok(())
    })
}

/// Draw an immediate button and activate on a primary press inside its rectangle.
/// # Safety
/// Text/event inputs are readable, output exclusively writable, for the whole call.
/// A null event draws without input. No borrowed storage is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_draw_button(
    handle: u64,
    rect: GuiRect,
    bytes: *const u8,
    length: u32,
    event: *const GuiEvent,
    activated: *mut u32,
) -> i32 {
    status((|| {
        pointer(activated, 1)?;
        // SAFETY: The caller guarantees readable inputs; copies precede state mutation.
        let event = unsafe { optional_event(event) }?;
        // SAFETY: The caller owns the bounded immutable input for this call.
        let label = unsafe { text(bytes, length) }?;
        let pressed = event.is_some_and(|event| {
            event.kind == 3
                && event.changed & event.buttons & 1 != 0
                && runtime::contains(rect, event.x, event.y)
        });
        let result = state(|runtime| {
            let mut canvas = runtime.canvas(handle)?;
            widget_rect(&mut canvas, rect, if pressed { 0x407080 } else { 0x304860 });
            canvas.text(
                rect.x.saturating_add(4),
                rect.y.saturating_add(5),
                label,
                0xffffff,
            );
            Ok(())
        });
        checked(result)?;
        // SAFETY: The label borrow is no longer used; caller owns the checked output.
        unsafe {
            activated.write(u32::from(pressed));
        }
        Ok(())
    })())
}

/// Edit caller-owned UTF-8 with Unicode-scalar backspace and render one text field.
/// # Safety
/// `bytes` is exclusively writable for capacity bytes, `length` exclusively writable,
/// and neither overlaps the other. Event is readable (or null) and copied first.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_draw_textfield(
    handle: u64,
    rect: GuiRect,
    bytes: *mut u8,
    capacity: u32,
    length: *mut u32,
    event: *const GuiEvent,
) -> i32 {
    status((|| {
        if capacity > MAX_TEXT_BYTES {
            return Err(Error::Limit);
        }
        pointer(length, 1)?;
        // SAFETY: The caller owns a live aligned length; no mutable reference exists yet.
        let mut count = unsafe { length.read() };
        if count > capacity {
            return Err(Error::Buffer);
        }
        pointer(bytes, capacity as usize)?;
        let data_start = bytes as usize;
        let data_end = data_start + capacity as usize;
        let length_start = length as usize;
        if data_start < length_start + 4 && length_start < data_end {
            return Err(Error::Invalid);
        }
        // SAFETY: The event is copied before taking the exclusive caller buffer borrow.
        let event = unsafe { optional_event(event) }?;
        // SAFETY: The caller supplies exclusive live storage and disjoint length output;
        // checked capacity bounds the slice and the event input has already been copied.
        let buffer = unsafe { core::slice::from_raw_parts_mut(bytes, capacity as usize) };
        let result = state(|runtime| {
            runtime.check(handle)?;
            edit_text(buffer, &mut count, event)?;
            let content =
                core::str::from_utf8(&buffer[..count as usize]).map_err(|_| Error::Utf8)?;
            let mut canvas = runtime.canvas(handle)?;
            widget_rect(&mut canvas, rect, 0xffffff);
            canvas.text(
                rect.x.saturating_add(4),
                rect.y.saturating_add(5),
                content,
                0x182838,
            );
            Ok(())
        });
        checked(result)?;
        // SAFETY: The data borrow has ended and the length output is checked/disjoint.
        unsafe {
            length.write(count);
        }
        Ok(())
    })())
}

fn edit_text(buffer: &mut [u8], length: &mut u32, event: Option<GuiEvent>) -> Result<(), Error> {
    let old = core::str::from_utf8(&buffer[..*length as usize]).map_err(|_| Error::Utf8)?;
    let Some(event) = event.filter(|event| event.kind == 2 && matches!(event.state, 1 | 3)) else {
        return Ok(());
    };
    if event.codepoint == 8 {
        if let Some((index, _)) = old.char_indices().next_back() {
            *length = index as u32;
        }
    } else if let Some(character) =
        char::from_u32(event.codepoint).filter(|value| !value.is_control())
    {
        let size = character.len_utf8();
        let start = *length as usize;
        if start + size > buffer.len() {
            return Err(Error::Buffer);
        }
        character.encode_utf8(&mut buffer[start..start + size]);
        *length += size as u32;
    }
    Ok(())
}

fn widget_rect(canvas: &mut render::Canvas<'_>, rect: GuiRect, fill: u32) {
    canvas.rect(rect, 0x788898);
    if rect.width > 2 && rect.height > 2 {
        canvas.rect(
            GuiRect {
                x: rect.x.saturating_add(1),
                y: rect.y.saturating_add(1),
                width: rect.width - 2,
                height: rect.height - 2,
            },
            fill,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn anova_gui_window_present(handle: u64) -> i32 {
    state(|runtime| runtime.present(handle))
}

/// Poll one ordered event without waiting; output remains unchanged on WouldBlock.
/// # Safety
/// `output` is an exclusively writable aligned live event.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_poll_event(handle: u64, output: *mut GuiEvent) -> i32 {
    if let Err(error) = pointer(output, 1) {
        return error as i32;
    }
    state(|runtime| {
        let event = runtime.next(handle)?;
        // SAFETY: The caller owns the checked event output for this complete call.
        unsafe {
            output.write(event);
        }
        Ok(())
    })
}

/// Allocate a vertical row, leaving both arguments unchanged on failure.
/// # Safety
/// `layout` and `output` are live exclusively writable aligned structures.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_layout_next(
    layout: *mut GuiLayout,
    output: *mut GuiRect,
) -> i32 {
    status((|| {
        pointer(layout, 1)?;
        pointer(output, 1)?;
        // SAFETY: Copy before either output write; no Rust reference aliases C storage.
        let mut value = unsafe { layout.read() };
        let rect = value.next_row()?;
        // SAFETY: The caller owns both checked live output ranges for this call.
        unsafe {
            layout.write(value);
            output.write(rect);
        }
        Ok(())
    })())
}

/// Copy into a process-private clipboard; data never crosses another application's boundary.
/// # Safety
/// `bytes` remains readable for length bytes (or may be null when length is zero).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_clipboard_set(bytes: *const u8, length: u32) -> i32 {
    // SAFETY: The caller guarantees a live bounded immutable source for this call.
    let value = match unsafe { text(bytes, length) } {
        Ok(value) => value,
        Err(error) => return error as i32,
    };
    state(|runtime| {
        runtime.clipboard[..value.len()].copy_from_slice(value.as_bytes());
        runtime.clipboard_length = value.len();
        Ok(())
    })
}

/// Copy the entire clipboard or return Buffer without modifying either output.
/// # Safety
/// `bytes` and `length` are live exclusively writable disjoint storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn anova_gui_clipboard_get(
    bytes: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> i32 {
    if capacity > MAX_TEXT_BYTES {
        return Error::Limit as i32;
    }
    if let Err(error) = pointer(length, 1).and_then(|()| pointer(bytes, capacity as usize)) {
        return error as i32;
    }
    state(|runtime| {
        if (capacity as usize) < runtime.clipboard_length {
            return Err(Error::Buffer);
        }
        // SAFETY: The caller owns the disjoint checked output spans; the static source
        // belongs only to the guarded runtime and no output pointer is retained.
        unsafe {
            core::ptr::copy_nonoverlapping(
                runtime.clipboard.as_ptr(),
                bytes,
                runtime.clipboard_length,
            );
            length.write(runtime.clipboard_length as u32);
        }
        Ok(())
    })
}

fn checked(code: i32) -> Result<(), Error> {
    match code {
        0 => Ok(()),
        -1 => Err(Error::Invalid),
        -2 => Err(Error::Version),
        -3 => Err(Error::Stale),
        -4 => Err(Error::Limit),
        -5 => Err(Error::Utf8),
        -6 => Err(Error::WouldBlock),
        -7 => Err(Error::Disconnected),
        -8 => Err(Error::Busy),
        -10 => Err(Error::Buffer),
        _ => Err(Error::Transport),
    }
}

/// Own the sole process-local window. The marker forbids moving it to another thread.
pub struct Window {
    handle: u64,
    affinity: PhantomData<*mut ()>,
}

impl Window {
    pub fn create(desc: GuiWindowDesc) -> Result<Self, Error> {
        let mut handle = 0;
        // SAFETY: Descriptor and exclusive output remain live and aligned for the call.
        checked(unsafe { anova_gui_window_create(&desc, &mut handle) })?;
        Ok(Self {
            handle,
            affinity: PhantomData,
        })
    }
    pub const fn handle(&self) -> u64 {
        self.handle
    }
    pub fn clear(&mut self, color: u32) -> Result<(), Error> {
        checked(anova_gui_window_clear(self.handle, color))
    }
    pub fn rect(&mut self, rect: GuiRect, color: u32) -> Result<(), Error> {
        checked(anova_gui_draw_rect(self.handle, rect, color))
    }
    pub fn text(&mut self, x: i32, y: i32, text: &str, color: u32) -> Result<(), Error> {
        let length = u32::try_from(text.len()).map_err(|_| Error::Limit)?;
        // SAFETY: The borrowed string remains immutable through the synchronous call.
        checked(unsafe { anova_gui_draw_text(self.handle, x, y, text.as_ptr(), length, color) })
    }
    pub fn button(
        &mut self,
        rect: GuiRect,
        label: &str,
        event: Option<&GuiEvent>,
    ) -> Result<bool, Error> {
        let length = u32::try_from(label.len()).map_err(|_| Error::Limit)?;
        let mut activated = 0;
        // SAFETY: Live immutable inputs and the exclusive local output are disjoint.
        checked(unsafe {
            anova_gui_draw_button(
                self.handle,
                rect,
                label.as_ptr(),
                length,
                event.map_or(core::ptr::null(), |value| value),
                &mut activated,
            )
        })?;
        Ok(activated != 0)
    }
    pub fn textfield(
        &mut self,
        rect: GuiRect,
        buffer: &mut [u8],
        length: &mut u32,
        event: Option<&GuiEvent>,
    ) -> Result<(), Error> {
        let capacity = u32::try_from(buffer.len()).map_err(|_| Error::Limit)?;
        // SAFETY: Rust exclusive borrows make the mutable buffer and length disjoint;
        // any event is immutably borrowed for the complete synchronous call.
        checked(unsafe {
            anova_gui_draw_textfield(
                self.handle,
                rect,
                buffer.as_mut_ptr(),
                capacity,
                length,
                event.map_or(core::ptr::null(), |value| value),
            )
        })
    }
    pub fn present(&mut self) -> Result<(), Error> {
        checked(anova_gui_window_present(self.handle))
    }
    pub fn poll(&mut self) -> Result<GuiEvent, Error> {
        let mut event = GuiEvent::EMPTY;
        // SAFETY: The event local remains exclusively writable until the call returns.
        checked(unsafe { anova_gui_poll_event(self.handle, &mut event) })?;
        Ok(event)
    }
    pub fn close(&mut self) -> Result<(), Error> {
        let handle = core::mem::replace(&mut self.handle, 0);
        checked(anova_gui_window_destroy(handle))
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        if self.handle != 0 {
            let _ = self.close();
        }
    }
}

pub fn query_version(requested: u32) -> Result<GuiVersion, Error> {
    let mut version = GuiVersion::default();
    // SAFETY: The local output remains exclusively owned and correctly aligned.
    checked(unsafe { anova_gui_query_version(requested, &mut version) })?;
    Ok(version)
}

pub fn clipboard_set(value: &str) -> Result<(), Error> {
    let length = u32::try_from(value.len()).map_err(|_| Error::Limit)?;
    // SAFETY: The string remains immutably borrowed through the synchronous copy.
    checked(unsafe { anova_gui_clipboard_set(value.as_ptr(), length) })
}

pub fn clipboard_get(buffer: &mut [u8]) -> Result<usize, Error> {
    let capacity = u32::try_from(buffer.len()).map_err(|_| Error::Limit)?;
    let mut length = 0;
    // SAFETY: The exclusive slice and local output are disjoint live ranges.
    checked(unsafe { anova_gui_clipboard_get(buffer.as_mut_ptr(), capacity, &mut length) })?;
    Ok(length as usize)
}

const _: () = assert!(size_of::<GuiVersion>() == 32 && align_of::<GuiVersion>() == 4);
const _: () = assert!(size_of::<GuiWindowDesc>() == 32 && align_of::<GuiWindowDesc>() == 4);
const _: () = assert!(size_of::<GuiRect>() == 16 && align_of::<GuiRect>() == 4);
const _: () = assert!(size_of::<GuiEvent>() == 48 && align_of::<GuiEvent>() == 4);
const _: () = assert!(size_of::<GuiLayout>() == 28 && align_of::<GuiLayout>() == 4);

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
