// SPDX-License-Identifier: Apache-2.0
//! Poll one fallibly allocated, pinned USB future on the BSP without blocking the scheduler.
//! The boot executor runs before users exist; runtime waits always return to the caller.
use alloc::{alloc::alloc, boxed::Box};
use core::{
    alloc::Layout,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

pub(super) type Task = Pin<Box<dyn Future<Output = ()>>>;

pub(super) fn allocate<F: Future<Output = ()> + 'static>(future: F) -> Result<Task, F> {
    let layout = Layout::new::<F>();
    assert!(layout.size() != 0, "USB task must own its controller");
    // SAFETY: Nonzero layout describes F exactly; null is handled before writing.
    let pointer = unsafe { alloc(layout) }.cast::<F>();
    if pointer.is_null() {
        return Err(future);
    }
    // SAFETY: Fresh aligned allocation holds one F. Box takes sole ownership with the
    // matching global allocator/layout. Pin prevents moves until its destructor runs.
    unsafe {
        pointer.write(future);
        Ok(Box::into_pin(Box::from_raw(pointer)))
    }
}

pub(super) fn boot<F: Future>(future: F) -> F::Output {
    let mut future = core::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        core::hint::spin_loop();
    }
}

/// A single scheduler boundary, independent of wakeups or heap allocation.
pub(super) async fn next_turn() {
    struct Yield(bool);
    impl Future for Yield {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                Poll::Pending
            }
        }
    }
    Yield(false).await;
}
