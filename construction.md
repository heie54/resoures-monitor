# Resource Monitor - 项目构建文档

## 项目概述

**项目名称**: Resource Monitor
**项目类型**: Tauri 2.x + Vue 3 桌面应用程序
**核心功能**: 实时监控系统资源（CPU、内存）和热门进程

## 技术栈

### 前端
| 技术 | 版本 | 用途 |
|------|------|------|
| Vue.js | 3.5.x | 前端框架（Composition API） |
| Vite | 8.x | 构建工具和开发服务器 |
| Chart.js | 4.4.x | 数据可视化（折线图） |
| @tauri-apps/api | 2.2.x | Tauri 前后端通信 |

### 后端
| 技术 | 版本 | 用途 |
|------|------|------|
| Rust | stable | 后端语言 |
| Tauri | 2.x | 桌面应用框架 |
| sysinfo | 0.29 | 系统信息收集（CPU、内存、进程） |
| serde | 1.0 | JSON 序列化 |
| tokio | 1.x | 异步运行时（已引入但未使用） |

### 构建工具
- **前端打包**: Vite（端口 1420）
- **桌面打包**: Tauri CLI（NSIS 安装程序）
- **Rust 工具链**: rustfmt, clippy, x86_64-pc-windows-msvc

## 项目结构

```
resource-monitor/
├── src/                          # Vue 前端源码
│   ├── main.js                   # Vue 入口文件
│   ├── App.vue                   # 主应用组件（仪表盘）
│   ├── style.css                 # 全局样式
│   └── components/              # 组件目录
│       └── HelloWorld.vue        # 示例组件（未使用）
├── src-tauri/                   # Rust 后端源码
│   ├── src/
│   │   └── main.rs              # 主程序文件（含所有逻辑）
│   ├── Cargo.toml               # Rust 依赖配置
│   ├── tauri.conf.json          # Tauri 应用配置
│   └── rust-toolchain.toml     # Rust 工具链配置
├── public/                      # 静态资源
├── index.html                   # HTML 入口
├── vite.config.js               # Vite 构建配置
├── package.json                 # NPM 依赖配置
└── tsconfig.json                # TypeScript 配置
```

## 模块设计

### 前端模块 (src/)

#### App.vue - 主仪表盘组件
**状态管理 (ref/reactive)**:
- `cpuData`, `memoryData` - 60点滚动历史数组
- `cpuUsage`, `memoryUsage` - 当前百分比值
- `topProcesses` - Top 3 进程数组
- `refreshInterval` - 刷新间隔（1000ms）

**核心方法**:
| 方法 | 作用 |
|------|------|
| `createChart(canvasRef, data, color)` | 创建 Chart.js 折线图（带渐变填充） |
| `updateChart(chart, dataArray)` | 更新图表数据（禁用动画） |
| `fetchData()` | 调用 `invoke('get_system_info')` 更新所有状态 |
| `formatCpu(value)` | CPU 格式化（1位小数） |
| `formatMemory(value)` | 内存格式化（1位小数） |
| `onMounted()` | 初始化图表，启动轮询 |
| `onUnmounted()` | 清理 interval，销毁图表 |

**UI 布局**:
- 深色主题背景（#0f0f0f，卡片 #1a1a1a）
- CSS Grid + Flexbox 布局
- 响应式断点 900px
- 颜色方案：CPU=#00ff88，Memory=#00aaff，Process=#ffaa00

#### 数据流
```
onMounted → setInterval(fetchData, 1000)
    ↓
invoke('get_system_info') → Rust 后端
    ↓
Response: { cpu_percent, memory_percent, top_processes }
    ↓
更新 ref 状态 → push 到历史数组 → updateChart()
```

### 后端模块 (src-tauri/)

#### main.rs - 所有后端逻辑（单文件架构）

**状态管理**:
```rust
struct AppState { system: Mutex<System> }  // sysinfo System 实例，线程安全
```

**Tauri 命令**:
```rust
#[tauri::command]
fn get_system_info(state: State<AppState>) -> Result<SystemInfo, String>
```

**返回值结构**:
```rust
struct SystemInfo {
    cpu_percent: f32,           // 全局 CPU 使用率 %
    memory_percent: f32,       // 内存使用率 %
    top_processes: Vec<ProcessInfo>,  // Top 3（按 CPU 排序）
}

struct ProcessInfo {
    name: String,              // 进程名称
    cpu_percent: f32,         // CPU 使用率
    memory_percent: f32,      // 内存使用率
}
```

**系统信息收集** (使用 sysinfo crate):
| 功能 | 实现 |
|------|------|
| CPU | `system.global_cpu_info().cpu_usage()` |
| 内存 | `(used_memory / total_memory) * 100.0` |
| 进程 | `system.processes()` 遍历 → 按 CPU 排序 → 取 Top 3 |

#### IPC 通信模式
- 前端调用: `invoke('get_system_info')`
- 后端响应: JSON 序列化（serde）
- 配置: 前端静态资源来自 `../dist`（Vite 开发服务器 localhost:5173）

## 前后端通信

```
┌─────────────────┐         Tauri IPC          ┌─────────────────┐
│   Vue Frontend   │ ──── invoke('get_system_info') ──── │   Rust Backend   │
│   (App.vue)     │                              │   (main.rs)      │
│                 │ ←──── { SystemInfo } ────── │                 │
│  - Chart.js     │                              │  - sysinfo       │
│  - 1s polling   │                              │  - Mutex<System> │
└─────────────────┘                              └─────────────────┘
```

## 构建配置

### Vite (vite.config.js)
- 开发服务器: 端口 1420
- Vue 插件: @vitejs/plugin-vue
- 目标: es2021, chrome100, safari15
- Sourcemaps: TAURI_DEBUG 模式下启用

### Tauri (tauri.conf.json)
- 窗口大小: 800x600（可调整）
- 标题: "Resource Monitor"
- Bundle: NSIS 安装程序
- 前端入口: `../dist`

## 关键实现点

1. **实时数据可视化**: Chart.js 折线图，60秒滚动窗口，1秒刷新
2. **跨平台系统监控**: sysinfo crate 提供统一的 CPU/内存/进程 API
3. **线程安全**: Mutex 保护共享状态，防止数据竞争
4. **轻量化**: 单文件后端，最小依赖，NSIS 单包分发
5. **响应式 UI**: 深色主题，移动端适配

## 模块间依赖关系

```
用户界面 (App.vue)
    ↓ invoke
Tauri Bridge (@tauri-apps/api)
    ↓ invoke('get_system_info')
Rust 后端 (main.rs)
    ↓
sysinfo crate → 系统调用
```

## 已知限制

1. **同步阻塞**: `get_system_info` 同步执行，Mutex 锁在整个收集过程
2. **无错误类型**: 使用 String 传播错误，缺少结构化错误处理
3. **无测试**: src-tauri/ 下无测试文件
4. **单命令架构**: 所有逻辑集中在 main.rs，无模块化拆分

## 快速开始

```bash
# 开发模式
npm run dev

# 构建前端
npm run build

# 构建桌面应用
npm run tauri build
```

---

*本文档由 AI 团队自动生成，用于项目理解快速上手*