<!-- Translate the M13 SDK contract and preserve exact commands and historical evidence. -->
# M13：应用 SDK 与包

[English](../../en/milestones/M13_SDK.md) | **简体中文**


M13 将原生 v1 运行时和独立 Linux-musl 开发环境整理成明确开发契约。[M13 验证](../validation/M13.md) 保存实际结果，[M12](M12_RUST.md) 和 [M10](M10_NETWORK_PACKAGES.md) 的平台限制继续有效。

## SDK 与 ABI 版本

SDK 初始语义版本为 1.0.0，目标 x86_64、anova.native.v1，与工具链目标 x86_64-unknown-linux-musl 及 linux.x86_64.v1 人格分离。原生 Rust 模板为 no_std，GNU 目标只提供 core 和普通用户代码生成，不代表原生 std。C/C++ 使用原生 CRT、调用封装和用户链接脚本；C++ 不含异常、RTTI、STL 或分配运行时，这些功能使用独立 M11/M12 Linux-musl 环境。

v1 调用编号、负错误、启动布局和 wire 结构冻结。SDK 可增加封装和文档，新可选调用在旧内核按原未支持错误失败；修改已有编号、结构、错误、调用或启动语义须新人格和独立兼容门禁。SDK 版本不是内核 ABI 版本。原生最多 64 个参数、每串 1023 字节，全部字符串、终止符和指针共同放入 32 KiB 启动区域，用户栈仍为 64 KiB。原生错误与 Linux errno 分离，Rust 类型化封装保留未知负值，C/C++ 使用相同稳定码；调用者须处理部分 I/O、EOF 和子进程故障。

## 交叉开发与独立导出

宿主使用锁定 Rust1.90.0、GCC/G++、binutils 和 xtask，先准备 [开发环境](../guides/DEVELOPMENT.md)，不将内核参数用于应用。完整创建、构建、打包、计算摘要、运行步骤保留下方。

app-run 检查独立外部摘要和真实包解析，将包放入镜像 `/repo/local.apkg`，打印带精确摘要的客户系统安装、检查和执行命令。串口执行后检查 status=0 并 shutdown。显式盘保留安装与数据，未选盘则建匹配 ABI 的临时盘，旧盘不重格式化；策略不匹配由客户系统安装器拒绝。app-image 只生成 anova-app.iso，app-test 通过相同公开路径验证实际执行、错误摘要拒绝和 C 模板的稳定缺文件错误。

升级需修改源码，将 project.toml 的整数 version 从 1 增至 2，重新构建、打包、计算摘要并复用同盘。安装后 pkg status 显示当前槽，第二次成功安装执行 `/persist/pkg/hello-c-b/file0`，后续交替；检查状态并干净重启验证保留。remove 保留 `/persist/data/hello-c`。cpp/rust 提供另外两种模板，Rust 含本地离线依赖及锁文件，使用 --offline --locked。

保留未剥离 app.elf、源码和摘要用于精确符号匹配。packer 验证源清单摘要，将临时载荷副本 strip 到 128 KiB 范围，恢复 ABI 尾部并重新验证，不改原调试 ELF。安装载荷与调试 ELF 摘要不同，但代码地址可对应；故障 PID/PC 本身没有构建摘要，匹配时须记录两者。

sdk-test 验证导出、三语言构建和 ELF，package-v2-test 另验打包后的真实执行。导出含头文件、运行时、ABI、链接/CRT、模板和每文件摘要，新候选完整才发布；旧导出出现额外文件或符号链接明确拒绝，不静默删除。独立使用时先复制不可变导出，再调用 Python 构建驱动；Rust 模板自带清单和离线锁，不依赖 app-new。不重新分发宿主编译器，也不声称编译器自举。

## 清单与准入

严格 app.toml 拒绝未知键、重复项、非法路径及不支持的 schema、SDK、ABI、架构、依赖、能力、权限。它记录整数包版本、入口和最低 SDK/musl/能力，当前许可证固定 Apache-2.0，没有任意依赖求解。

ANOVAPK2 为小端、最多 128 KiB，头 256 字节，最多四个 96 字节描述符；V1 读取兼容保留。

| 偏移 | 字段 |
| --- | --- |
| 0 / 8 / 36 | ANOVAPK2 魔数、名称、递增版本 |
| 40 / 44 / 48 | SDK 主版本、文件数量、x86_64 架构 |
| 64 / 96 / 112 | 显式 ABI、许可证、包支持范围 |
| 128 / 132 | 清单修订号、精确头大小 |
| 136 / 140 / 144 | 权限、最低能力、入口文件名 |
| 192 / 224 | 依赖名称、最低依赖版本 |

保留字节必须为零。描述符包含安全单分量名称、模式、精确长度和 SHA-256，LICENSE 强制存在。V2 可执行载荷还含摘要覆盖的 ABI 元数据，误换其他目标文件会拒绝；这些保证完整性与一致性，不认证发布者或签名。

挂载时 `/persist/.anova-abi` 为权威策略，VFS 固定其 inode，挂载期间拒绝写入、截断、chmod、执行位修改、删除和双向重命名，读取仍可用。改变策略需另建盘。原生盘收原生包，Linux 盘只接有界静态 ET_EXEC 包；动态/PIE 包准入后置，但 M9 更宽运行时不缩减。安装在暂存前检查，不猜 OSABI、解释器或调用者人格，不支持混 ABI 持久盘，initramfs 仍为逐条身份。

权限只是单用户准入声明，不形成文件或网络沙箱；未知值默认拒绝。M13 接受读/数据权限、VFS/持久存储及 Linux 包所需能力，网络和 GUI 服务需求没有准入契约则拒绝，不能用能力声明授权未实现功能。

## 安装、升级与数据

经受控镜像、仓库或受控 HTTPS 传包时，同时传来自可信源的整包 SHA-256。install-file 先核外部锁和全包再写，pkg check 只验格式与完整性，安装另查盘策略；内部摘要不能认证来源。

非活跃槽文件和清单写入并 fsync 后，发布校验代际记录选择新版本。入口对应 file0..3，清单保原名称、模式和摘要。升级须增版本，暂存失败保旧活跃版，删除发布墓碑；数据仅放 `/persist/data/NAME`，跨升级和卸载保留，不能放安装槽。无日志 ext2 没有断电事务保证。package-v2-test 覆盖清单解析负例、人格不匹配、真实执行、升级失败、干净重启和数据保留，旧 package-test/TLS 门禁继续保留。

M11/M12 的客户系统 GCC/G++、Rust/std、Cargo 使用 Linux-musl 和明确 Linux 盘，不代表原生 libc/std。离线用 --offline --frozen，联网明确选择独立范围，宿主交叉编译与系统内编译分别验收。

## 日志、符号与资源诊断

UART 日志和 shell status 区分正常返回、包拒绝及隔离进程故障。内核记录 PID、向量、指令和栈及故障地址；捕获硬件故障后、原生终止或 Linux 信号处理前，受检复制最多 16 个用户帧，RSP 上方跨度最多 1 MiB，相邻链接最多 64 KiB。非法、未对齐、循环或越界即停止，优化或缺帧指针可导致部分回溯。

SDK 保调试符号和帧指针，symbolize 必须使用精确未剥离 ELF；多进程日志用 --pid，拒绝把无关 PID 合到同一 ELF。PIE/动态代码需真实装载偏移与匹配对象，不代表任意动态符号化，内核故障处理不解析 DWARF。资源快照为全局空闲页、堆和任务容量，不是单进程统计，应对照 32 槽、16 MiB 堆和文件/地址空间上限。smoke/OOM 验所有权恢复，日志标记不能证明编译器完成。GDB stub、交互调试器、通用 DWARF 展开和编译器自举仍独立。

## 独立 GUI SDK

M14.4 已实现 Rust 运行时和独立 anova.gui.v1 C ABI、C 声明、C++ 仅移动 RAII、Rust 安全封装，版本、布局、句柄、生命周期、错误、线程和不展开约定见 [M14 运行时](M14_RUNTIME.md) 与 [GUI SDK](../../../sdk/gui/README.zh-CN.md)。它有独立源码摘要和链接执行元数据。

原生 SDK1.0.0、ANOVAPK2 准入仍独立。GUI 应用静态链接运行时，在明确原生卷构建执行；ANOVAPK2 没有动态 GUI 服务依赖准入，M13 仍拒绝未声明 GUI 要求。不代表原生 libc/std、公共 Linux GUI 或进程沙箱。

## 原始命令与输出

以下命令、源码与日志保留原文；执行位置及通过条件见上文。

```sh
cargo xtask fetch
cargo xtask doctor
cargo xtask sdk
cargo xtask app-new c build/examples/hello-c
cargo xtask app-build build/examples/hello-c
cargo xtask app-pack build/examples/hello-c/app.toml build/examples/hello-c/hello-c.apkg
cargo xtask root-image build/disks/sdk-native.ext2
APP_SHA=$(sha256sum build/examples/hello-c/hello-c.apkg | cut -d' ' -f1)
cargo xtask app-run build/examples/hello-c/hello-c.apkg "$APP_SHA" --disk build/disks/sdk-native.ext2
```

```sh
cp -R build/sdk/anova-sdk-1.0.0 build/sdk-consumer
python3 build/sdk-consumer/build.py rust build/sdk-consumer/templates/rust
```

```sh
cargo xtask diagnostics-test
cargo xtask symbolize build/diagnostics/m13-crash.elf 0x40002b
# Replace 3 with the matching PID recorded in your fault log.
cargo xtask symbolize build/diagnostics/m13-crash.elf --log build/logs/diagnostics-guest.serial.log --pid 3
```
