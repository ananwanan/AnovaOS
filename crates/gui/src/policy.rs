//! Apply authenticated window operations and route physical input in bounded user space.
//!
//! One caller serializes all calls. Four registered clients each own at most one window;
//! its private fixed-size pixel partition never aliases another client. The caller supplies
//! backing storage and a retained framebuffer. Upload checks finish before any pixel writes.
//! The damage union is conservative and recomposes every intersecting layer after occlusion.
//!
//! SPDX-License-Identifier: Apache-2.0

use crate::{CREATE_VISIBLE, Error, Reply, Request, VERSION, op, reply_kind};

pub const MAX_CLIENTS: usize = 4;
pub const MAX_WINDOWS: usize = 4;
pub const MAX_WIDTH: u32 = 320;
pub const MAX_HEIGHT: u32 = 240;
pub const MIN_WIDTH: u32 = 32;
pub const MIN_HEIGHT: u32 = 24;
pub const MAX_SURFACE_PIXELS: usize = MAX_WIDTH as usize * MAX_HEIGHT as usize;
pub const STORAGE_PIXELS: usize = MAX_WINDOWS * MAX_SURFACE_PIXELS;
pub const MAX_UPLOAD_PIXELS: usize = (4096 - crate::MESSAGE_BYTES) / 4;
pub const BACKGROUND: u32 = 0x18283b;
pub const TITLE_HEIGHT: u32 = 16;
pub const ACTIVE_TITLE: u32 = 0x354d66;
pub const INACTIVE_TITLE: u32 = 0x2b3d52;
pub const ACTIVE_BORDER: u32 = 0xe0e8f0;
pub const INACTIVE_BORDER: u32 = 0x728397;
pub type ClientId = u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(self, x: i32, y: i32) -> bool {
        i64::from(x) >= i64::from(self.x)
            && i64::from(y) >= i64::from(self.y)
            && i64::from(x) < i64::from(self.x) + i64::from(self.width)
            && i64::from(y) < i64::from(self.y) + i64::from(self.height)
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let left = i64::from(self.x).max(i64::from(other.x));
        let top = i64::from(self.y).max(i64::from(other.y));
        let right = (i64::from(self.x) + i64::from(self.width))
            .min(i64::from(other.x) + i64::from(other.width));
        let bottom = (i64::from(self.y) + i64::from(self.height))
            .min(i64::from(other.y) + i64::from(other.height));
        (left < right && top < bottom).then(|| {
            Self::new(
                left as i32,
                top as i32,
                (right - left) as u32,
                (bottom - top) as u32,
            )
        })
    }

    fn union(self, other: Self) -> Self {
        // Damage rectangles have already been clipped to the bounded screen.
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width as i32).max(other.x + other.width as i32);
        let bottom = (self.y + self.height as i32).max(other.y + other.height as i32);
        Self::new(left, top, (right - left) as u32, (bottom - top) as u32)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowInfo {
    pub handle: u64,
    pub client: ClientId,
    pub rect: Rect,
    pub visible: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutedEvent {
    pub client: ClientId,
    pub reply: Reply,
}

#[derive(Clone, Copy)]
struct Slot {
    generation: u32,
    handle: u64,
    client: ClientId,
    rect: Rect,
    visible: bool,
}

impl Slot {
    const EMPTY: Self = Self {
        generation: 0,
        handle: 0,
        client: 0,
        rect: Rect::new(0, 0, 0, 0),
        visible: false,
    };
}

#[derive(Clone, Copy)]
struct Drag {
    handle: u64,
    x: i32,
    y: i32,
    rect: Rect,
    resize: bool,
}

pub struct Compositor<'a> {
    width: u32,
    height: u32,
    session: u64,
    clients: [ClientId; MAX_CLIENTS],
    slots: [Slot; MAX_WINDOWS],
    order: [usize; MAX_WINDOWS],
    order_len: usize,
    focus: Option<usize>,
    capture: Option<usize>,
    drag: Option<Drag>,
    pointer_x: i32,
    pointer_y: i32,
    pointer_visible: bool,
    damage: Option<Rect>,
    storage: &'a mut [u32],
}

impl<'a> Compositor<'a> {
    pub fn new(
        width: u32,
        height: u32,
        session: u64,
        storage: &'a mut [u32],
    ) -> Result<Self, Error> {
        if width == 0
            || height == 0
            || width > anova_graphics::MAX_DIMENSION
            || height > anova_graphics::MAX_DIMENSION
            || width as usize * height as usize > anova_graphics::MAX_PIXELS
        {
            return Err(Error::InvalidSurface);
        }
        if session == 0 {
            return Err(Error::Session);
        }
        let storage = storage
            .get_mut(..STORAGE_PIXELS)
            .ok_or(Error::BufferTooSmall)?;
        storage.fill(0);
        Ok(Self {
            width,
            height,
            session,
            clients: [0; MAX_CLIENTS],
            slots: [Slot::EMPTY; MAX_WINDOWS],
            order: [0; MAX_WINDOWS],
            order_len: 0,
            focus: None,
            capture: None,
            drag: None,
            pointer_x: (width / 2) as i32,
            pointer_y: (height / 2) as i32,
            pointer_visible: false,
            damage: Some(Rect::new(0, 0, width, height)),
            storage,
        })
    }

    pub const fn session(&self) -> u64 {
        self.session
    }

    /// The identity must come from the authenticated kernel envelope, never request bytes.
    /// Repeated registration is idempotent, allowing a transport to retry its first packet.
    pub fn connect(&mut self, client: ClientId) -> Result<(), Error> {
        if client == 0 {
            return Err(Error::UnknownClient);
        }
        if self.clients.contains(&client) {
            return Ok(());
        }
        let entry = self
            .clients
            .iter_mut()
            .find(|entry| **entry == 0)
            .ok_or(Error::Quota)?;
        *entry = client;
        Ok(())
    }

    /// Drop every private surface and pointer capture for an exited/disconnected process.
    pub fn disconnect(&mut self, client: ClientId) -> bool {
        let Some(entry) = self
            .clients
            .iter()
            .position(|entry| *entry == client && client != 0)
        else {
            return false;
        };
        for index in 0..MAX_WINDOWS {
            if self.slots[index].client == client {
                self.destroy(index);
            }
        }
        self.clients[entry] = 0;
        true
    }

    pub fn connected_clients(&self) -> usize {
        self.clients.iter().filter(|client| **client != 0).count()
    }

    pub const fn window_count(&self) -> usize {
        self.order_len
    }

    pub fn focused_client(&self) -> Option<ClientId> {
        self.focus.map(|index| self.slots[index].client)
    }

    pub fn focused_window(&self) -> Option<u64> {
        self.focus.map(|index| self.slots[index].handle)
    }

    pub fn client_window(&self, client: ClientId) -> Option<WindowInfo> {
        self.slots
            .iter()
            .find(|slot| slot.client == client && slot.handle != 0)
            .and_then(|slot| self.window_info(slot.handle))
    }

    pub fn window_info(&self, handle: u64) -> Option<WindowInfo> {
        let index = self.index(handle).ok()?;
        let slot = self.slots[index];
        Some(WindowInfo {
            handle,
            client: slot.client,
            rect: slot.rect,
            visible: slot.visible,
            focused: self.focus == Some(index),
        })
    }

    pub const fn pending_damage(&self) -> Option<Rect> {
        self.damage
    }

    pub fn apply(&mut self, client: ClientId, request: &Request, pixels: &[u32]) -> Reply {
        match self.apply_checked(client, request, pixels) {
            Ok(handle) => {
                let mut reply = Reply::success(request.sequence, self.session);
                reply.window = handle;
                if let Some(info) = self.window_info(handle) {
                    reply.x = info.rect.x;
                    reply.y = info.rect.y;
                    reply.width = info.rect.width;
                    reply.height = info.rect.height;
                    reply.flags = u32::from(info.visible);
                }
                reply
            }
            Err(error) => Reply::failure(request.sequence, self.session, error),
        }
    }

    fn apply_checked(
        &mut self,
        client: ClientId,
        request: &Request,
        pixels: &[u32],
    ) -> Result<u64, Error> {
        if request.version != VERSION {
            return Err(Error::Version);
        }
        if request.session != self.session {
            return Err(Error::Session);
        }
        if request.reserved != 0 || request.flags & !CREATE_VISIBLE != 0 {
            return Err(Error::Malformed);
        }
        if client == 0 || !self.clients.contains(&client) {
            return Err(Error::UnknownClient);
        }
        if request.opcode != op::UPLOAD
            && (!pixels.is_empty() || request.pixel_count != 0 || request.stride != 0)
        {
            return Err(Error::BufferLength);
        }
        if request.opcode != op::CREATE && request.flags != 0 {
            return Err(Error::Malformed);
        }
        match request.opcode {
            op::DESTROY | op::SHOW | op::HIDE | op::FOCUS
                if request.x != 0
                    || request.y != 0
                    || request.width != 0
                    || request.height != 0 =>
            {
                return Err(Error::Malformed);
            }
            op::MOVE if request.width != 0 || request.height != 0 => {
                return Err(Error::Malformed);
            }
            op::RESIZE if request.x != 0 || request.y != 0 => {
                return Err(Error::Malformed);
            }
            _ => {}
        }
        if request.opcode == op::CREATE {
            return self.create(client, request);
        }
        if !(op::DESTROY..=op::PRESENT).contains(&request.opcode) {
            return Err(Error::UnknownOpcode);
        }
        let index = self.index(request.window)?;
        if self.slots[index].client != client {
            return Err(Error::ForeignHandle);
        }
        match request.opcode {
            op::DESTROY => self.destroy(index),
            op::MOVE => {
                let old = self.slots[index].rect;
                self.slots[index].rect.x = request.x;
                self.slots[index].rect.y = request.y;
                self.mark_window(index, old);
            }
            op::RESIZE => self.resize(index, request.width, request.height)?,
            op::SHOW => {
                self.slots[index].visible = true;
                self.raise_focus(index);
                self.mark(self.slots[index].rect);
            }
            op::HIDE => {
                self.mark(self.slots[index].rect);
                self.slots[index].visible = false;
                self.drop_capture(index);
                if self.focus == Some(index) {
                    self.select_focus();
                }
            }
            op::FOCUS => {
                if !self.slots[index].visible {
                    return Err(Error::InvalidSurface);
                }
                self.raise_focus(index);
            }
            op::UPLOAD => self.upload(index, request, pixels)?,
            op::PRESENT => {
                let slot = self.slots[index];
                let area = if request.width == 0 && request.height == 0 {
                    if request.x != 0 || request.y != 0 {
                        return Err(Error::InvalidSurface);
                    }
                    slot.rect
                } else {
                    self.local_rect(index, request)?;
                    Rect::new(
                        slot.rect.x.saturating_add(request.x),
                        slot.rect.y.saturating_add(request.y),
                        request.width,
                        request.height,
                    )
                };
                if slot.visible {
                    self.mark(area);
                }
            }
            _ => return Err(Error::UnknownOpcode),
        }
        Ok(request.window)
    }

    fn create(&mut self, client: ClientId, request: &Request) -> Result<u64, Error> {
        valid_size(request.width, request.height)?;
        if request.window != 0 {
            return Err(Error::Malformed);
        }
        if self.slots.iter().any(|slot| slot.client == client) {
            return Err(Error::Quota);
        }
        let index = self
            .slots
            .iter()
            .position(|slot| slot.handle == 0)
            .ok_or(Error::Quota)?;
        let generation = self.slots[index]
            .generation
            .checked_add(1)
            .ok_or(Error::Arithmetic)?;
        let handle = (u64::from(generation) << 8) | (index as u64 + 1);
        self.slots[index] = Slot {
            generation,
            handle,
            client,
            rect: Rect::new(request.x, request.y, request.width, request.height),
            visible: request.flags & CREATE_VISIBLE != 0,
        };
        self.clear(index);
        self.order[self.order_len] = index;
        self.order_len += 1;
        if self.slots[index].visible {
            self.raise_focus(index);
            self.mark(self.slots[index].rect);
        }
        Ok(handle)
    }

    fn index(&self, handle: u64) -> Result<usize, Error> {
        let low = (handle & 0xff) as usize;
        if low == 0 || low > MAX_WINDOWS || self.slots[low - 1].handle != handle {
            return Err(Error::StaleHandle);
        }
        Ok(low - 1)
    }

    fn local_rect(&self, index: usize, request: &Request) -> Result<(), Error> {
        let rect = self.slots[index].rect;
        if request.width == 0
            || request.height == 0
            || request.x < 0
            || request.y < 0
            || (request.x as u32)
                .checked_add(request.width)
                .is_none_or(|x| x > rect.width)
            || (request.y as u32)
                .checked_add(request.height)
                .is_none_or(|y| y > rect.height)
        {
            return Err(Error::InvalidSurface);
        }
        Ok(())
    }

    fn upload(&mut self, index: usize, request: &Request, pixels: &[u32]) -> Result<(), Error> {
        self.local_rect(index, request)?;
        if request.stride < request.width {
            return Err(Error::InvalidSurface);
        }
        let required = request
            .stride
            .checked_mul(request.height - 1)
            .and_then(|prefix| prefix.checked_add(request.width))
            .ok_or(Error::Arithmetic)? as usize;
        if request.pixel_count as usize != pixels.len()
            || pixels.len() != required
            || pixels.len() > MAX_UPLOAD_PIXELS
        {
            return Err(Error::BufferLength);
        }
        let start = index * MAX_SURFACE_PIXELS;
        for y in 0..request.height as usize {
            for x in 0..request.width as usize {
                let destination =
                    start + (request.y as usize + y) * MAX_WIDTH as usize + request.x as usize + x;
                self.storage[destination] = pixels[y * request.stride as usize + x] & 0x00ff_ffff;
            }
        }
        Ok(())
    }

    fn resize(&mut self, index: usize, width: u32, height: u32) -> Result<(), Error> {
        valid_size(width, height)?;
        let old = self.slots[index].rect;
        if old.width != width || old.height != height {
            self.slots[index].rect.width = width;
            self.slots[index].rect.height = height;
            self.clear(index);
            self.mark_window(index, old);
        }
        Ok(())
    }

    fn clear(&mut self, index: usize) {
        self.storage[index * MAX_SURFACE_PIXELS..(index + 1) * MAX_SURFACE_PIXELS].fill(0);
    }

    fn destroy(&mut self, index: usize) {
        self.mark(self.slots[index].rect);
        self.drop_capture(index);
        self.clear(index);
        self.slots[index].handle = 0;
        self.slots[index].client = 0;
        self.slots[index].visible = false;
        if let Some(position) = self.order[..self.order_len]
            .iter()
            .position(|entry| *entry == index)
        {
            self.order
                .copy_within(position + 1..self.order_len, position);
            self.order_len -= 1;
        }
        if self.focus == Some(index) {
            self.select_focus();
        }
    }

    fn drop_capture(&mut self, index: usize) {
        if self.capture == Some(index) {
            self.capture = None;
            self.drag = None;
        }
    }

    fn select_focus(&mut self) {
        self.focus = self.order[..self.order_len]
            .iter()
            .rev()
            .copied()
            .find(|index| self.slots[*index].visible);
        if let Some(index) = self.focus {
            self.mark(self.slots[index].rect);
        }
    }

    fn raise_focus(&mut self, index: usize) {
        if self.focus != Some(index) {
            self.capture = None;
            self.drag = None;
        }
        if let Some(old) = self.focus {
            self.mark(self.slots[old].rect);
        }
        if let Some(position) = self.order[..self.order_len]
            .iter()
            .position(|entry| *entry == index)
        {
            self.order
                .copy_within(position + 1..self.order_len, position);
            self.order[self.order_len - 1] = index;
        }
        self.focus = Some(index);
        self.mark(self.slots[index].rect);
    }

    fn mark_window(&mut self, index: usize, old: Rect) {
        if self.slots[index].visible {
            self.mark(old);
            self.mark(self.slots[index].rect);
        }
    }

    fn mark(&mut self, rect: Rect) {
        if let Some(clipped) = rect.intersection(Rect::new(0, 0, self.width, self.height)) {
            self.damage = Some(self.damage.map_or(clipped, |damage| damage.union(clipped)));
        }
    }

    fn cursor_rect(&self) -> Rect {
        Rect::new(
            self.pointer_x,
            self.pointer_y,
            anova_graphics::CURSOR_WIDTH,
            anova_graphics::CURSOR_HEIGHT,
        )
    }

    /// Mouse coordinates are clipped to the screen. Buttons use PS/2 low-three-bit values.
    /// Press raises/focuses; capture retains the target through button release. Title dragging
    /// and bottom-right resizing belong here in user space, with no kernel window policy.
    pub fn pointer(&mut self, x: i32, y: i32, buttons: u32, changed: u32) -> Option<RoutedEvent> {
        if buttons & !7 != 0 || changed & !7 != 0 {
            return self.input_reset();
        }
        if self.pointer_visible {
            self.mark(self.cursor_rect());
        }
        self.pointer_x = x.clamp(0, self.width as i32 - 1);
        self.pointer_y = y.clamp(0, self.height as i32 - 1);
        self.pointer_visible = true;
        self.mark(self.cursor_rect());
        let x = self.pointer_x;
        let y = self.pointer_y;
        let hit = self.order[..self.order_len]
            .iter()
            .rev()
            .copied()
            .find(|index| self.slots[*index].visible && self.slots[*index].rect.contains(x, y));
        if buttons & changed != 0
            && self.capture.is_none()
            && let Some(index) = hit
        {
            self.raise_focus(index);
            self.capture = Some(index);
            let rect = self.slots[index].rect;
            let local_x = (i64::from(x) - i64::from(rect.x)) as u32;
            let local_y = (i64::from(y) - i64::from(rect.y)) as u32;
            if buttons & changed & 1 != 0
                && (local_y < TITLE_HEIGHT
                    || (local_x >= rect.width - 12 && local_y >= rect.height - 12))
            {
                self.drag = Some(Drag {
                    handle: self.slots[index].handle,
                    x,
                    y,
                    rect,
                    resize: local_y >= TITLE_HEIGHT,
                });
            }
        }
        let target = self.capture.or(hit);
        if let Some(drag) = self.drag
            && let Ok(index) = self.index(drag.handle)
        {
            let dx = x - drag.x;
            let dy = y - drag.y;
            if drag.resize {
                let width = (i64::from(drag.rect.width) + i64::from(dx))
                    .clamp(i64::from(MIN_WIDTH), i64::from(MAX_WIDTH))
                    as u32;
                let height = (i64::from(drag.rect.height) + i64::from(dy))
                    .clamp(i64::from(MIN_HEIGHT), i64::from(MAX_HEIGHT))
                    as u32;
                let _ = self.resize(index, width, height);
            } else {
                let old = self.slots[index].rect;
                self.slots[index].rect.x = drag
                    .rect
                    .x
                    .saturating_add(dx)
                    .clamp(-(drag.rect.width as i32 - 16), self.width as i32 - 16);
                self.slots[index].rect.y = drag
                    .rect
                    .y
                    .saturating_add(dy)
                    .clamp(0, self.height as i32 - 1);
                self.mark_window(index, old);
            }
        }
        if buttons & 1 == 0 {
            self.drag = None;
        }
        if buttons == 0 {
            self.capture = None;
        }
        let index = target?;
        let slot = self.slots[index];
        let mut reply = Reply::success(0, self.session);
        reply.kind = reply_kind::POINTER;
        reply.window = slot.handle;
        reply.x = x.saturating_sub(slot.rect.x);
        reply.y = y.saturating_sub(slot.rect.y);
        reply.width = slot.rect.width;
        reply.height = slot.rect.height;
        reply.value = buttons;
        reply.flags = changed;
        Some(RoutedEvent {
            client: slot.client,
            reply,
        })
    }

    /// Physical key numbers and modifiers are independent from text/layout policy.
    pub fn key(&self, code: u32, state: u32, modifiers: u32) -> Option<RoutedEvent> {
        if code > 383 || !(1..=3).contains(&state) || modifiers & !15 != 0 {
            return None;
        }
        let index = self.focus?;
        let mut reply = Reply::success(0, self.session);
        reply.kind = reply_kind::KEY;
        reply.window = self.slots[index].handle;
        reply.width = state;
        reply.value = code;
        reply.flags = modifiers;
        Some(RoutedEvent {
            client: self.slots[index].client,
            reply,
        })
    }

    pub fn input_reset(&mut self) -> Option<RoutedEvent> {
        self.capture = None;
        self.drag = None;
        if self.pointer_visible {
            self.mark(self.cursor_rect());
        }
        self.pointer_visible = false;
        let index = self.focus?;
        let mut reply = Reply::success(0, self.session);
        reply.kind = reply_kind::RESET;
        reply.window = self.slots[index].handle;
        Some(RoutedEvent {
            client: self.slots[index].client,
            reply,
        })
    }

    /// Repaint the clipped damage union in caller-retained tightly packed screen pixels.
    /// No writes occur on a short buffer; errors preserve pending damage for a retry.
    pub fn render(&mut self, output: &mut [u32]) -> Result<Option<Rect>, Error> {
        let count = self.width as usize * self.height as usize;
        let output = output.get_mut(..count).ok_or(Error::BufferTooSmall)?;
        let Some(damage) = self.damage else {
            return Ok(None);
        };
        for y in damage.y as usize..damage.y as usize + damage.height as usize {
            let start = y * self.width as usize + damage.x as usize;
            output[start..start + damage.width as usize].fill(BACKGROUND);
        }
        for position in 0..self.order_len {
            let index = self.order[position];
            let slot = self.slots[index];
            if !slot.visible {
                continue;
            }
            let Some(area) = slot.rect.intersection(damage) else {
                continue;
            };
            for y in area.y..area.y + area.height as i32 {
                for x in area.x..area.x + area.width as i32 {
                    let sx = (i64::from(x) - i64::from(slot.rect.x)) as usize;
                    let sy = (i64::from(y) - i64::from(slot.rect.y)) as usize;
                    let active = self.focus == Some(index);
                    let color = if sx == 0
                        || sy == 0
                        || sx + 1 == slot.rect.width as usize
                        || sy + 1 == slot.rect.height as usize
                    {
                        if active {
                            ACTIVE_BORDER
                        } else {
                            INACTIVE_BORDER
                        }
                    } else if sy < TITLE_HEIGHT as usize {
                        if active { ACTIVE_TITLE } else { INACTIVE_TITLE }
                    } else {
                        self.storage[index * MAX_SURFACE_PIXELS + sy * MAX_WIDTH as usize + sx]
                    };
                    output[y as usize * self.width as usize + x as usize] = color;
                }
            }
        }
        if self.pointer_visible
            && let Some(area) = self.cursor_rect().intersection(damage)
        {
            for y in area.y..area.y + area.height as i32 {
                for x in area.x..area.x + area.width as i32 {
                    if let Some(color) = anova_graphics::cursor_color(
                        (x - self.pointer_x) as u32,
                        (y - self.pointer_y) as u32,
                    ) {
                        output[y as usize * self.width as usize + x as usize] = color;
                    }
                }
            }
        }
        self.damage = None;
        Ok(Some(damage))
    }
}

fn valid_size(width: u32, height: u32) -> Result<(), Error> {
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) || !(MIN_HEIGHT..=MAX_HEIGHT).contains(&height) {
        return Err(Error::InvalidSurface);
    }
    let pixels = width.checked_mul(height).ok_or(Error::Arithmetic)? as usize;
    if pixels > MAX_SURFACE_PIXELS {
        return Err(Error::Quota);
    }
    Ok(())
}
