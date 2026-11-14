// SPDX-License-Identifier: Apache-2.0
//! Assert native monitor copy, task ownership, wait retention and real allocator deltas.
//! The host gate checks disk counts independently and exercises the visible desktop controls.

use super::{gui, rt};
use rt::abi::{monitor as m, *};

fn raw(op: u64, target: u64, bytes: usize) -> i64 {
    // SAFETY: Deliberately invalid addresses are used only with checked monitor operations.
    unsafe { rt::raw(m::SYSCALL, op, target, bytes as u64) }
}

pub(super) fn run() {
    let me = rt::info().unwrap().process_id;
    let idle = rt::monitor::resources().unwrap();
    let mut tasks = [m::Task::default(); m::MAX_TASKS];
    let own = rt::monitor::tasks(&mut tasks).unwrap();
    assert_eq!(own.len(), 1);
    assert_eq!(
        (own[0].pid, own[0].kind, own[0].state),
        (me, m::NATIVE, m::RUNNING)
    );
    assert!(tasks[1..].iter().all(|task| task.pid == 0));
    assert_eq!(raw(2 << 32, 0, 0), INVALID_ARGUMENT);
    assert_eq!(raw(m::command(m::STOP), me, 1), INVALID_ARGUMENT);
    assert_eq!(
        raw(m::command(m::RESOURCES), 0, m::RESOURCE_BYTES - 1),
        INVALID_ARGUMENT
    );
    assert_eq!(
        raw(m::command(m::RESOURCES), 0, m::RESOURCE_BYTES),
        BAD_ADDRESS
    );
    assert_eq!(
        raw(
            m::command(m::TASKS),
            run as *const () as u64,
            m::MAX_TASKS * m::TASK_BYTES
        ),
        BAD_ADDRESS
    );
    assert_eq!(
        raw(
            m::command(m::TASKS),
            USER_LIMIT - 16,
            m::MAX_TASKS * m::TASK_BYTES
        ),
        BAD_ADDRESS
    );
    assert_eq!(rt::monitor::stop(0), Err(INVALID_ARGUMENT));
    assert_eq!(rt::monitor::stop(1), Err(NO_CHILD));
    assert_eq!(rt::monitor::stop(me), Err(NO_CHILD));

    let child = rt::spawn("desktop", "park").unwrap();
    let started = rt::monitor::tasks(&mut tasks)
        .unwrap()
        .iter()
        .find(|task| task.pid == child)
        .copied()
        .unwrap();
    assert_eq!(started.flags, m::CAN_STOP);
    assert_eq!(started.parent, me);
    let mut ids = [0; m::MAX_TASKS];
    let mut n = 0;
    loop {
        match rt::spawn("desktop", "park") {
            Ok(pid) => {
                ids[n] = pid;
                n += 1;
            }
            Err(PROCESS_LIMIT) => break,
            Err(error) => panic!("Unexpected pressure error {error}"),
        }
    }
    let full = rt::monitor::resources().unwrap();
    assert_eq!(full.tasks_total, 32);
    assert_eq!(full.tasks.iter().sum::<u32>(), 32);
    assert!(full.frames_free < idle.frames_free && full.heap_used > idle.heap_used);
    for id in core::iter::once(&child).chain(&ids[..n]) {
        assert_eq!(rt::monitor::stop(*id), Ok(true));
        assert_eq!(rt::monitor::stop(*id), Ok(false));
        let zombie = rt::monitor::tasks(&mut tasks)
            .unwrap()
            .iter()
            .find(|task| task.pid == *id)
            .copied()
            .unwrap();
        assert_eq!(zombie.state, m::ZOMBIE);
        assert_eq!(
            zombie.exit,
            ExitStatus {
                reason: 3,
                code: 15
            }
        );
        assert_eq!(rt::wait(*id).unwrap().status, zombie.exit);
    }
    let restored = rt::monitor::resources().unwrap();
    assert_eq!(restored.frames_free, idle.frames_free);
    assert_eq!(restored.heap_used, idle.heap_used);
    assert_eq!(restored.opens_used, idle.opens_used);
    assert_eq!(
        restored.tasks.iter().sum::<u32>(),
        idle.tasks.iter().sum::<u32>()
    );

    let replacement = rt::spawn("desktop", "park").unwrap();
    assert!(replacement > child);
    assert_eq!(rt::monitor::stop(child), Err(NO_CHILD));
    let linux = rt::spawn("/linux/desktop-park", "").unwrap();
    assert_eq!(rt::monitor::stop(linux), Err(UNSUPPORTED));
    let children = rt::monitor::tasks(&mut tasks).unwrap();
    assert!(
        children
            .iter()
            .any(|task| task.pid == replacement && task.flags == m::CAN_STOP)
    );
    assert!(
        children
            .iter()
            .any(|task| task.pid == linux && task.kind == m::LINUX_PROCESS && task.flags == 0)
    );
    // SAFETY: There are no owned GUI or PTY objects here. The forked peer only checks
    // its own access rights, exits and is waited on by this process.
    let peer = unsafe { rt::fork() }.unwrap();
    if peer == 0 {
        let own = rt::monitor::tasks(&mut tasks).unwrap();
        assert_eq!(own.len(), 1);
        assert_ne!(own[0].pid, me);
        assert_eq!(rt::monitor::stop(replacement), Err(NO_CHILD));
        rt::exit(0);
    }
    assert_eq!(rt::wait(peer).unwrap().status.code, 0);
    assert_eq!(rt::wait(linux).unwrap().status.code, 0);
    assert!(rt::monitor::stop(replacement).unwrap());
    assert_eq!(rt::wait(replacement).unwrap().status.code, 15);
    let disk = rt::monitor::resources().unwrap();
    let path = "/persist/monitor-check.tmp";
    let fd = rt::open(path, OPEN_CREATE | OPEN_EXCLUSIVE | OPEN_WRITE).unwrap();
    rt::write_all(fd, &[b'x'; 4096]).unwrap();
    let used = rt::monitor::resources().unwrap();
    assert_eq!(used.inodes_free + 1, disk.inodes_free);
    assert_eq!(used.blocks_free + 1, disk.blocks_free);
    assert_eq!(used.opens_used, disk.opens_used + 1);
    assert_eq!(used.nodes_used, disk.nodes_used + 1);
    rt::unlink(path).unwrap();
    assert_eq!(
        rt::monitor::resources().unwrap().inodes_free,
        used.inodes_free
    );
    rt::close(fd).unwrap();
    let after = rt::monitor::resources().unwrap();
    assert_eq!(
        (after.inodes_free, after.blocks_free),
        (disk.inodes_free, disk.blocks_free)
    );
    assert_eq!(after.opens_used, disk.opens_used);
    assert_eq!(after.nodes_used, disk.nodes_used);

    let mut masters = [0; 4];
    for fd in &mut masters {
        *fd = rt::open("/dev/ptmx", OPEN_READ | OPEN_WRITE).unwrap();
    }
    assert_eq!(
        rt::open("/dev/ptmx", OPEN_READ | OPEN_WRITE),
        Err(FILE_LIMIT)
    );
    let ptys = rt::monitor::resources().unwrap();
    assert_eq!(
        (ptys.ptys_total, ptys.ptys_used, ptys.ptys_managed),
        (4, 4, 0)
    );
    for fd in masters {
        rt::close(fd).unwrap();
    }
    let ptys = rt::monitor::resources().unwrap();
    assert_eq!(
        (ptys.ptys_used, ptys.opens_used),
        (after.ptys_used, after.opens_used)
    );
    gui::log(format_args!(
        "[MONITOR:PASS] tasks={} frames_free={} heap_used={} opens={} nodes={} slots={} ptys={} gui={} disk={} blocks={} free_blocks={} inodes={} free_inodes={}\n",
        full.tasks.iter().sum::<u32>(),
        restored.frames_free,
        restored.heap_used,
        after.opens_used,
        after.nodes_used,
        after.node_slots,
        ptys.ptys_used,
        after.gui_used,
        after.disk_format,
        after.blocks_total,
        after.blocks_free,
        after.inodes_total,
        after.inodes_free,
    ));
}
