<!-- Navigate the complete bilingual documentation tree and distinguish current status from historical evidence. -->
# AnovaOS documentation

**English** | [简体中文](../zh-CN/index.md)

This index covers matching English and Simplified Chinese designs, development guides and historical acceptance evidence. Local profiles cover M0–M13, M14.1–M14.5 and M15.1–M15.6. M15.3/M15.4 are compiler feasibility assessments with blockers; M15.5/M15.6 still use host-assisted kernel image packaging.

Planned follow-ups are M16 resources/VFS/ext4, M17 development desktop and conditional M18
self-hosting. M16 now has a locally verified backend/cache and controlled ext4 readonly slice;
writes/recovery/adoption remain open. M17/M18 remain planned; see the roadmap and M16 evidence.

[Project README](../../README.md) · [Development rules](../../AGENTS.md) · [Current status](guides/STATUS.md) · [Roadmap](ROADMAP.md)

## Project design

- [PROJECT_OUTLINE.md](PROJECT_OUTLINE.md)
- [ARCHITECTURE.md](ARCHITECTURE.md)
- [ROADMAP.md](ROADMAP.md)
- [COMPATIBILITY.md](COMPATIBILITY.md)

## Guides

- [CODING_STYLE.md](guides/CODING_STYLE.md)
- [DEVELOPMENT.md](guides/DEVELOPMENT.md)
- [FILE_PURPOSES.md](guides/FILE_PURPOSES.md)
- [FIRST_TASK.md](guides/FIRST_TASK.md)
- [GITHUB_SETUP.md](guides/GITHUB_SETUP.md)
- [LICENSING.md](guides/LICENSING.md)
- [REFERENCES.md](guides/REFERENCES.md)
- [REQUIREMENTS.md](guides/REQUIREMENTS.md)
- [STATUS.md](guides/STATUS.md)
- [TOOLCHAINS.md](guides/TOOLCHAINS.md)

## Milestone designs

- [M10_NETWORK_PACKAGES.md](milestones/M10_NETWORK_PACKAGES.md)
- [M11_TOOLCHAIN.md](milestones/M11_TOOLCHAIN.md)
- [M12_NETWORK.md](milestones/M12_NETWORK.md)
- [M12_RUST.md](milestones/M12_RUST.md)
- [M13_SDK.md](milestones/M13_SDK.md)
- [M14_COMPOSITOR.md](milestones/M14_COMPOSITOR.md)
- [M14_GPU.md](milestones/M14_GPU.md)
- [M14_GUI.md](milestones/M14_GUI.md)
- [M14_RUNTIME.md](milestones/M14_RUNTIME.md)
- [M15_BOOTSTRAP.md](milestones/M15_BOOTSTRAP.md)
- [M15_GCC.md](milestones/M15_GCC.md)
- [M15_KERNEL.md](milestones/M15_KERNEL.md)
- [M15_REPEAT.md](milestones/M15_REPEAT.md)
- [M15_RUST.md](milestones/M15_RUST.md)
- [M16_FOUNDATION.md](milestones/M16_FOUNDATION.md)
- [M17_DESKTOP.md](milestones/M17_DESKTOP.md)
- [M2_CPU.md](milestones/M2_CPU.md)
- [M3_MEMORY.md](milestones/M3_MEMORY.md)
- [M4_USERSPACE.md](milestones/M4_USERSPACE.md)
- [M5_PROCESSES.md](milestones/M5_PROCESSES.md)
- [M6_FILESYSTEM.md](milestones/M6_FILESYSTEM.md)
- [M7_STORAGE.md](milestones/M7_STORAGE.md)
- [M8_LINUX.md](milestones/M8_LINUX.md)
- [M9_RUNTIME.md](milestones/M9_RUNTIME.md)

## Validation

These records retain their original dates, hashes and historical bounds. Documentation maintenance does not rerun guest gates. Remote CI and physical-hardware results are recorded separately.

- [M0-M1.md](validation/M0-M1.md)
- [M10.md](validation/M10.md)
- [M11.md](validation/M11.md)
- [M12.md](validation/M12.md)
- [M13.md](validation/M13.md)
- [M14.1.md](validation/M14.1.md)
- [M14.2.md](validation/M14.2.md)
- [M14.3.md](validation/M14.3.md)
- [M14.4.md](validation/M14.4.md)
- [M14.5.md](validation/M14.5.md)
- [M15.1-M15.2.md](validation/M15.1-M15.2.md)
- [M15.3.md](validation/M15.3.md)
- [M15.4.md](validation/M15.4.md)
- [M15.5.md](validation/M15.5.md)
- [M15.6.md](validation/M15.6.md)
- [M16.md](validation/M16.md)
- [M2.md](validation/M2.md)
- [M3.md](validation/M3.md)
- [M4.md](validation/M4.md)
- [M5.md](validation/M5.md)
- [M6.md](validation/M6.md)
- [M7.md](validation/M7.md)
- [M8.md](validation/M8.md)
- [M8_OPTIMIZATION.md](validation/M8_OPTIMIZATION.md)
- [M9.md](validation/M9.md)

## Third-party inputs and notices

Context is translated; upstream legal text remains verbatim in both languages and is not replaced by a legal translation.

- [m10.md](third-party/m10.md)
- [m11.md](third-party/m11.md)
- [m12.md](third-party/m12.md)
- [m15.md](third-party/m15.md)
- [musl.md](third-party/musl.md)
- [network-licenses.md](third-party/network-licenses.md)

## Component entry points

- [Native SDK](../../sdk/README.md)
- [GUI SDK](../../sdk/gui/README.md)
- [Reconstruction recipes](../../ports/bootstrap/README.md)

## Documentation maintenance

Update both READMEs and both document trees in the same batch when interfaces, commands, limits, status or next steps change. Shared images/diagrams belong in `docs/assets/images/` and `docs/assets/diagrams/`; commit resources when actual assets exist.

```sh
python3 scripts/check_docs.py
cargo xtask check-files
git diff --check
```
