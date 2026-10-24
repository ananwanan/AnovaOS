// SPDX-License-Identifier: Apache-2.0
//! Exercise compositor-only GPU readback, checked admission and CPU recovery on a retained display.
//! This bounded diagnostic does not change the ordinary software window composition policy.

use anova_gui::transport as t;
use anova_gui_user as gui;
use anova_rt::abi::*;

fn clear(request: &t::GpuClear) -> Result<(), i64> {
    // SAFETY: The kernel validates all pointers, including deliberately invalid test requests.
    // Valid output arrays remain exclusively owned and live for this synchronous call;
    // the initialized request has no padding and the kernel retains no user address.
    unsafe {
        gui::call(
            t::GPU_CLEAR,
            request as *const _ as u64,
            size_of::<t::GpuClear>(),
        )
    }
    .map(|_| ())
}

pub fn run(fail: bool) -> u32 {
    // Authorization is checked before any argument or device access.
    // SAFETY: Null/zero are deliberate rejected arguments; there is no memory to borrow.
    match unsafe { gui::call(t::GPU_CLEAR, 0, 0) } {
        Err(PERMISSION_DENIED) => {}
        Err(UNSUPPORTED) => {
            gui::log(format_args!(
                "[GPU:USER-UNAVAILABLE] kernel_feature=false\n"
            ));
            return 2;
        }
        _ => panic!("GPU compositor authorization failed"),
    }
    gui::scalar(t::BIND).unwrap();
    let mut pixels = [0xdead_beef_u32; 64 * 64];
    let mut request = t::GpuClear {
        pixels: pixels.as_mut_ptr() as u64,
        width: 64,
        height: 64,
        rgb: 0xff0000,
        reserved: 0,
    };
    for (width, height, rgb, reserved) in [
        (0, 64, 0, 0),
        (65, 64, 0, 0),
        (64, u32::MAX, 0, 0),
        (64, 64, 0x1000000, 0),
        (64, 64, 0, 1),
    ] {
        let bad = t::GpuClear {
            width,
            height,
            rgb,
            reserved,
            ..request
        };
        assert_eq!(clear(&bad), Err(INVALID_ARGUMENT));
    }
    let bad = t::GpuClear {
        pixels: u64::MAX - 8,
        ..request
    };
    assert_eq!(clear(&bad), Err(BAD_ADDRESS));
    // SAFETY: Exact size mismatch must be rejected before any access to the live request.
    assert_eq!(
        unsafe { gui::call(t::GPU_CLEAR, &request as *const _ as u64, 23) },
        Err(INVALID_ARGUMENT)
    );
    assert!(pixels.iter().all(|p| *p == 0xdead_beef));
    gui::log(format_args!(
        "[GPU:USER-BOUNDS] authorization=true dimensions=true format=true pointer=true unchanged=true\n"
    ));
    let mut software = fail;
    if fail {
        assert_eq!(gui::scalar(t::GPU_TEST_TIMEOUT), Err(DEVICE_TIMEOUT));
        gui::log(format_args!(
            "[GPU:USER-FAILURE] timeout=true fallback=cpu\n"
        ));
    } else {
        let odd = t::GpuClear {
            width: 7,
            height: 5,
            rgb: 0x1280fe,
            ..request
        };
        match clear(&odd) {
            Ok(()) => {
                assert!(pixels[..35].iter().all(|p| *p == 0x1280fe));
                assert!(pixels[35..].iter().all(|p| *p == 0xdead_beef));
                gui::log(format_args!(
                    "[GPU:USER-ODD] width=7 height=5 rgb=001280fe pixels=35 canary=true\n"
                ));
            }
            Err(error @ (UNSUPPORTED | NOT_FOUND | DEVICE_TIMEOUT | IO_ERROR | NO_MEMORY)) => {
                gui::log(format_args!("[GPU:USER-FALLBACK] error={error}\n"));
                software = true;
            }
            Err(_) => panic!("GPU odd-size clear admission failed"),
        }
    }
    for rgb in [0xff0000, 0x0000ff, 0xffffff, 0x00ff00] {
        request.rgb = rgb;
        pixels.fill(0xdead_beef);
        if !software {
            match clear(&request) {
                Ok(()) => {}
                Err(error @ (UNSUPPORTED | NOT_FOUND | DEVICE_TIMEOUT | IO_ERROR | NO_MEMORY)) => {
                    gui::log(format_args!("[GPU:USER-FALLBACK] error={error}\n"));
                    software = true;
                }
                Err(_) => panic!("GPU clear admission failed"),
            }
        }
        if software {
            pixels.fill(rgb);
        }
        assert!(
            pixels.iter().all(|p| *p == rgb),
            "GPU output differs from CPU reference"
        );
        let scanout = t::Scanout {
            pixels: pixels.as_ptr() as u64,
            x: 16,
            y: 16,
            width: 64,
            height: 64,
            stride: 64,
            reserved: 0,
        };
        // SAFETY: The initialized descriptor and all 4096 private pixels remain readable
        // for the synchronous checked copy; PRESENT never retains their addresses.
        unsafe { gui::input(t::PRESENT, &scanout) }.unwrap();
    }
    gui::log(format_args!(
        "[GPU:USER] backend={} pixels=4096 clears=4\n",
        if software { "cpu" } else { "virgl" }
    ));
    gui::scalar(t::UNBIND).unwrap();
    gui::log(format_args!("[GPU:USER-DONE]\n"));
    0
}
