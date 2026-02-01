###  重构方案报告
核心目标 ：将扁平的代码结构转换为模块化结构，分离关注点。
 1. 📂 目录结构调整
我们将创建 5 个新的子模块目录，并将现有文件进行归类和迁移：

- backend/ (后端与系统交互)
  
  - 负责处理底层系统交互，如 DRM/KMS 显示后端、Wayland Socket 和 IPC 通信。
  - udev.rs -> backend/udev.rs
  - socket.rs -> backend/socket.rs
  - ipc_server.rs -> backend/ipc.rs (建议重命名为 ipc 更简洁)
- shell/ (窗口与界面管理)
  
  - 负责所有与窗口、层级、概览和 XWayland 相关的逻辑。
  - layer.rs -> shell/layer.rs
  - overview.rs -> shell/overview.rs
  - windows/ (目录) -> shell/windows/
  - xwayland/ (目录) -> shell/xwayland/
- output/ (显示与渲染)
  
  - 负责屏幕输出、绘图逻辑和屏幕方向处理。
  - output.rs -> output/mod.rs (或者保持 output.rs 放入目录)
  - drawing.rs -> output/drawing.rs
  - orientation.rs -> output/orientation.rs
- input/ (输入处理)
  
  - 负责键盘、鼠标、触摸等输入设备的逻辑。
  - input.rs -> input/mod.rs (或者 input.rs 放入目录)
- utils/ (通用工具)
  
  - 存放非业务逻辑的通用代码。
  - geometry.rs -> utils/geometry.rs
- 核心状态文件重命名
  
  - catacomb.rs -> state.rs
  - 理由 ： catacomb.rs 目前承载了 Catacomb 结构体，这是合成器的核心状态（Compositor State）。将其命名为 state.rs 在 Rust 项目中是更标准的做法，能清晰表明它持有全局状态。 2. 🌳 重构后的文件树概览
```
apps/catacomb/src/
├── main.rs                 # 程序入口与 CLI 解析
├── config.rs               # 配置文件处理
├── state.rs                # [原 catacomb.rs] 核心 Compositor 状态
├── utils/                  # [新增] 工具模块
│   ├── mod.rs
│   └── geometry.rs         # [原 geometry.rs]
├── backend/                # [新增] 系统后端模块
│   ├── mod.rs
│   ├── udev.rs             # [原 udev.rs]
│   ├── socket.rs           # [原 socket.rs]
│   └── ipc.rs              # [原 ipc_server.rs]
├── input/                  # [新增] 输入模块
│   └── mod.rs              # [原 input.rs]
├── output/                 # [新增] 输出与渲染模块
│   ├── mod.rs              # [原 output.rs 内容]
│   ├── drawing.rs          # [原 drawing.rs]
│   └── orientation.rs      # [原 orientation.rs]
├── shell/                  # [新增] 窗口管理模块
│   ├── mod.rs
│   ├── layer.rs            # [原 layer.rs]
│   ├── overview.rs         # [原 overview.rs]
│   ├── windows/            # [原 windows 目录]
│   └── xwayland/           # [原 xwayland 目录]
└── protocols/              # [保持不变] 协议实现
```
### ✅ 方案优势
1. 高内聚低耦合 ：
   - 修改渲染逻辑时只需关注 output/ 目录。
   - 修改窗口管理策略时只需关注 shell/ 目录。
2. 更清晰的依赖关系 ：
   - main.rs 负责组装。
   - state.rs 负责持有数据。
   - 各个子模块负责具体的行为实现。
3. 符合 Rust 惯例 ：利用模块系统 ( mod.rs ) 来组织代码，不仅目录清晰，而且在代码中引用 ( use crate::shell::windows ) 也更符合语义。
如果您对这个方案满意，我将开始执行以下步骤：

1. 创建目录结构。
2. 移动文件并重命名。
3. 最重要的一步 ：系统性地修复代码中所有的 use crate::... 引用路径，确保编译通过。
请确认是否开始执行？