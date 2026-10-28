<!-- Navigate the complete bilingual documentation tree and distinguish current status from historical evidence. -->
# AnovaOS 文档

[English](../en/index.md) | **简体中文**

本索引覆盖两种语言的阶段设计、开发指南与历史验收证据。当前受测范围覆盖 M0–M13、M14.1–M14.5 与 M15.1–M15.6；M15.3/M15.4 是有阻塞结论的编译器可行性评估，M15.5/M15.6 的内核镜像仍由宿主辅助打包。

后续计划为 M16 资源/VFS/ext4、M17 开发桌面、M18 有条件自举，M16 的后端/缓存和受控 ext4 只读部分已本地验证，
受控写入、JBD2 恢复、迁移和声明的内核容量已有本地证据，远端 CI 待验。M17.1 的有界五启动配置已完成物理输入、故障重启、资源复用及文件保留本地验收；M17.2 的真实 PTY/GCC/重启已本地验收，M17.3 已实现最小文件/编辑闭环；完整存储故障验收、M17.4–M17.5/M18 待完成，详见路线图和各阶段证据。

[项目 README](../../README.zh-CN.md) · [开发规范](../../AGENTS.md) · [当前状态](guides/STATUS.md) · [路线图](ROADMAP.md)

## 总体设计

- [PROJECT_OUTLINE.md](PROJECT_OUTLINE.md)
- [ARCHITECTURE.md](ARCHITECTURE.md)
- [ROADMAP.md](ROADMAP.md)
- [COMPATIBILITY.md](COMPATIBILITY.md)

## 开发指南

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

## 阶段设计

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
- [M17_PTY.md](milestones/M17_PTY.md)
- [M17_FILES.md](milestones/M17_FILES.md)
- [M2_CPU.md](milestones/M2_CPU.md)
- [M3_MEMORY.md](milestones/M3_MEMORY.md)
- [M4_USERSPACE.md](milestones/M4_USERSPACE.md)
- [M5_PROCESSES.md](milestones/M5_PROCESSES.md)
- [M6_FILESYSTEM.md](milestones/M6_FILESYSTEM.md)
- [M7_STORAGE.md](milestones/M7_STORAGE.md)
- [M8_LINUX.md](milestones/M8_LINUX.md)
- [M9_RUNTIME.md](milestones/M9_RUNTIME.md)

## 验收记录

以下是原阶段执行记录，日期、哈希与历史限制不因文档整理而改变；它们不代表本次重新运行。远端 CI 与实机结果单独记录。

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
- [M16.4.md](validation/M16.4.md)
- [M16.5.md](validation/M16.5.md)
- [M16.6.md](validation/M16.6.md)
- [M16_CAPACITY.md](validation/M16_CAPACITY.md)
- [M17.1.md](validation/M17.1.md)
- [M17.2.md](validation/M17.2.md)
- [M17.3.md](validation/M17.3.md)
- [M2.md](validation/M2.md)
- [M3.md](validation/M3.md)
- [M4.md](validation/M4.md)
- [M5.md](validation/M5.md)
- [M6.md](validation/M6.md)
- [M7.md](validation/M7.md)
- [M8.md](validation/M8.md)
- [M8_OPTIMIZATION.md](validation/M8_OPTIMIZATION.md)
- [M9.md](validation/M9.md)

## 第三方输入与声明

说明文字提供中文版本，上游许可正文在两种语言中均原样保留，不作为译本替换正式条款。

- [m10.md](third-party/m10.md)
- [m11.md](third-party/m11.md)
- [m12.md](third-party/m12.md)
- [m15.md](third-party/m15.md)
- [musl.md](third-party/musl.md)
- [network-licenses.md](third-party/network-licenses.md)

## 组件入口

- [原生 SDK](../../sdk/README.zh-CN.md)
- [GUI SDK](../../sdk/gui/README.zh-CN.md)
- [重建配方](../../ports/bootstrap/README.zh-CN.md)

## 文档维护

接口、命令、限制、状态或下一步变化时，在同一开发批次更新双语 README 和两套文档。共享图片/图表新增于 `docs/assets/images/` 和 `docs/assets/diagrams/`，仅在有实际素材时提交。

```sh
python3 scripts/check_docs.py
cargo xtask check-files
git diff --check
```
