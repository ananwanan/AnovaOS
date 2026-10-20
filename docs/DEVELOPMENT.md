# 开发环境与验证约定

## 当前可用内容

仓库目前只有 Markdown 规划、git 忽略和文本格式配置。尚无 Cargo workspace、内核、镜像生成器和 CI；本页中的工程命令除明确标明外均为计划接口。

当前可以阅读文档并将 [FIRST_TASK](FIRST_TASK.md) 交给 Codex 开始 M0/M1。不要运行不存在的脚本，也不要用预设测试输出代替真实结果。

日常开发以 `dev` 为基线，任务分支从 `dev` 创建，Pull Request 以 `dev` 为目标；`main` 接收阶段验收后的稳定快照。克隆时可以显式指定 `--branch dev`，避免工作区停留在旧分支。

## 宿主环境

首个受测宿主选择 x86_64 Linux；Windows 用户可通过 WSL2 构建并采用 QEMU TCG。KVM 作为可选加速，不能作为 CI 或普通启动的强制前提。后续根据真实验证增加 Windows 原生/macOS 宿主说明。

M0 需要核对并记录：Rust/rustup/Cargo、裸机 target、QEMU x86_64、OVMF 固件、Limine 及协议绑定、LLD/LLVM 工具、镜像制作工具、Python 等实际采用组件。镜像方案决定是否需要 xorriso、mtools 等，不一次性添加无用依赖。

在 M0 实际验证之前，不编造 Rust nightly 日期、crate 版本、OVMF 文件路径或 apt 包名组合；发行版路径必须由 doctor 检测或配置。

## 计划命令契约

由 M0/M1 实现以下命令。实现前不能声称运行成功。

| 命令 | 应完成的工作 | 失败行为 |
| --- | --- | --- |
| `cargo xtask doctor` | 检查依赖、版本与固件/引导器输入 | 缺失项逐项说明，返回非零 |
| `cargo xtask build` | 构建内核及需要的自有用户组件 | 编译/链接失败原样传播 |
| `cargo xtask image` | 由受控输入生成 UEFI 启动镜像 | 输入缺失、校验失败时停止 |
| `cargo xtask run` | QEMU 启动；默认临时数据盘与固件变量 | 不自动 sudo，不访问真实磁盘 |
| `cargo xtask smoke` | 无交互启动、标记验证、超时和退出结果解码 | panic/超时/错误退出判为失败 |
| `cargo xtask test-host` | 运行已存在的纯逻辑宿主测试 | 不把不能运行的内核测试算通过 |

后续按需加入 app 构建、持久化测试、compat 测试、sysroot 和打包子命令；每增加入口先实现再写入使用说明。

## target 隔离

- workspace 的宿主工具用宿主 target 构建，不设置一个会污染所有 crate 的裸机默认目标。
- kernel 的 linker script、code model、panic、CPU features 单独配置。
- native app 和 Linux 兼容 app 各有独立 target、runtime 与 sysroot，日志打印实际配置。
- 纯逻辑库在宿主测试；需要硬件/内核语义的用例放 guest 测试。
- Cargo.lock、构建配方、外部输入清单与校验和提交 git；生成缓存和镜像忽略。

## 测试与日志

系统测试采用清楚的阶段标记，runner 还必须验证最终状态和退出结果。例如 `[BOOT:OK]` 可以表示完成 M1，但不能用于证明内存隔离或应用开发。标记名在实现中统一，不能用文本替代实际执行。

使用测试专用 QEMU 退出设备时，记录宿主观察到的退出码变换；不能简单要求 QEMU 返回 0。测试模式与交互模式分开，正常交互启动不应自动退出。

持久盘验证使用专用用例：启动 → 创建/修改文件 → flush → 正常关机 → 再启动 → 比较内容。默认 snapshot/overlay 适合普通试运行，但不能证明持久化。

## 验证记录格式

每个里程碑在 `docs/validation/` 按实际需要创建记录：

- 内核 commit、工作区是否有修改、日期。
- 宿主 OS、Rust/引导器/QEMU/OVMF 版本、关键依赖锁定值。
- 启动配置、内存、CPU、vCPU 数、设备、测试盘输入。
- 实际命令、退出码、关键日志和产物哈希。
- 成功/失败/未执行的测试与原因，以及支持范围。

不提交密钥、个人目录、真实磁盘内容或大型无关日志。二进制发布产物按发布流程提供，不塞进源码提交。

## CI 引入时机

文档阶段没有必须运行的 Rust CI。M0/M1 命令可复现后再加格式/构建/适当宿主测试/QEMU 冒烟工作流；有条件依赖时明确区分未运行和通过。

关键检查失败要阻止通过，不能用 `|| true`、虚构成功日志或 `continue-on-error` 规避。没有新改动或新风险时，无需重复扩张验证范围。
