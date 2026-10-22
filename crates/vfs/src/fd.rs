// SPDX-License-Identifier: Apache-2.0
//! Own fallibly allocated shared descriptor tables and allocation-free empty placeholders.
//! Vfs manages open-description references; sharing a live table never duplicates its opens.
use anova_abi::{BAD_DESCRIPTOR, FILE_LIMIT, NO_MEMORY};
use anova_memory::shared::Shared;
use core::cell::RefCell;
pub const FD_LIMIT: usize = 32;
#[derive(Clone, Copy)]
pub(crate) struct Table {
    pub close_on_exec: [bool; FD_LIMIT],
    pub slots: [Option<usize>; FD_LIMIT],
}
impl Table {
    fn empty() -> Self {
        Self {
            close_on_exec: [false; FD_LIMIT],
            slots: [None; FD_LIMIT],
        }
    }
}
#[derive(Default)]
pub struct Descriptors {
    pub(crate) table: Option<Shared<RefCell<Table>>>,
}
impl Descriptors {
    pub(crate) fn ensure(&mut self) -> Result<(), i64> {
        if self.table.is_none() {
            self.table =
                Some(Shared::try_new(RefCell::new(Table::empty())).map_err(|_| NO_MEMORY)?);
        }
        Ok(())
    }
    pub fn share(&mut self) -> Result<Self, i64> {
        self.ensure()?;
        Ok(Self {
            table: self.table.clone(),
        })
    }
    pub(crate) fn shared(&self) -> bool {
        self.table
            .as_ref()
            .is_some_and(|t| Shared::strong_count(t) > 1)
    }
    pub(crate) fn from_table(table: Table) -> Result<Self, i64> {
        Ok(Self {
            table: Some(Shared::try_new(RefCell::new(table)).map_err(|_| NO_MEMORY)?),
        })
    }
    pub(crate) fn slots(&self) -> [Option<usize>; FD_LIMIT] {
        self.table
            .as_ref()
            .map_or([None; FD_LIMIT], |t| t.borrow().slots)
    }
    pub fn set_close_on_exec(&mut self, fd: u64, value: bool) -> Result<(), i64> {
        self.handle(fd)?;
        self.table.as_ref().unwrap().borrow_mut().close_on_exec[fd as usize] = value;
        Ok(())
    }
    pub fn close_on_exec(&self, fd: u64) -> Result<bool, i64> {
        self.handle(fd)?;
        Ok(self.table.as_ref().unwrap().borrow().close_on_exec[fd as usize])
    }
    pub fn count(&self) -> usize {
        self.slots().iter().flatten().count()
    }
    pub fn contains(&self, fd: u64) -> bool {
        self.handle(fd).is_ok()
    }
    pub(crate) fn handle(&self, fd: u64) -> Result<usize, i64> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.slots().get(i).copied())
            .flatten()
            .ok_or(BAD_DESCRIPTOR)
    }
    pub(crate) fn free(&self) -> Result<usize, i64> {
        self.slots()
            .iter()
            .position(Option::is_none)
            .ok_or(FILE_LIMIT)
    }
}
