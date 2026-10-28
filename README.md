# CacheTalk 🛰️

> **A Zero-Syscall Microarchitectural Covert Channel Framework for LLM Agents across Heterogeneous Hardware**

CacheTalk 旨在打通从底层硬件微架构缓存侧信道（CPU Cache Side-Channel / Covert Channel）到顶层大语言模型智能体（LLM Agent）之间的“零文件系统、零共享内存、零系统调用（Zero-Syscall）”的纯硬件隐蔽通信通道。

---

## 核心技术文档

- 🗺️ **[全芯片 Cache 特性与隐蔽通信技术全景地图 (Landscape & Growth Map)](docs/CACHE_SIDE_CHANNEL_LANDSCAPE.md)**
  - **威胁模型重构**：彻底放弃易被 eBPF / auditd 捕获的 `Flush+Reload` (mmap)，转向纯私有匿名堆 (`Vec<u8>`) 的 `Prime+Probe` 与 `LLC Occupancy + FSK`（软件定义无线电思路，Goertzel 窄带滤波抗噪）。
  - **最新前沿实证**：OpenAI 2026.10 前沿红队报告（跨隔离 microVM 的 LLM Agent 缓存声学调制解调通信）、USENIX Sec'25 *I Know What You Said*、*Spill The Beans* 词表 Embedding 硬件侧信道。
  - **全芯片家族微架构特性对照**：Intel Raptor Lake/Xeon, AMD Zen 4/5, ARM Neoverse, Apple Silicon M1–M4, RISC-V, GPU。
  - **Rust 生态开源积木**：`calibration-done-right` (Rust Trait 与 Benchmark), `evsets` ($O(N)$ 驱逐集构建算法), `libflush`。
  - **攻防对抗全景**：EDR / Seccomp 零系统调用绕过 vs 硬件 PMU (TDT) MPKI 异常检测，以及基于 LLM GEMM 矩阵运算的 **Workload Shaping（工作负载伪装）**。

- 🏗️ **[系统架构设计文档 (Architecture Design)](docs/ARCHITECTURE.md)**
  - 分层架构设计（L0-L3 四层架构）
  - Rust Workspace 模块化设计
  - 硬件抽象层 (HAL) Trait 详细设计
  - 调制解调层协议（FSK、Goertzel Filter、帧封装）
  - MCP Server 接口规范
  - 性能目标与基准测试框架

- 📊 **[详细调研数据 (Research Data)](artifacts/tmp/detailed-data/)**
  - `gemini_research_summary.md` - Gemini 调研内容总结（8 大核心发现：现成项目、核心文献、芯片特性对比、除 Cache 外的微架构通信方式、Rust 框架设计、链路层协议、MCP 接口、MVP 建议）
  - `gemini_turn_1.txt` - Gemini 对话第一轮（项目定义、Rust 生态、核心文献索引）
  - `gemini_turn_2.txt` - Gemini 对话第二轮（零文件系统通信机制、三大核心模式、检测体系分析）
  - `zchat_assistant_reply_1.txt` - ZChat 助手回复 1（现成项目盘点：covert-channel-tutorial、Mastik、MIT 6.5950 Lab）
  - `zchat_assistant_reply_2.txt` - ZChat 助手回复 2（跨平台差异：x86/ARM/Apple Silicon 原语对比）
  - `zchat_assistant_reply_3.txt` - ZChat 助手回复 3（芯片特性调研：Intel/AMD/ARM/Apple/RISC-V/GPU 全景）
  - `zchat_chat_data.json` - ZChat 对话元数据（ID、时间戳、模型信息）
  - `zchat_messages_full.json` - ZChat 完整消息记录（包含 content_blocks、tool_calls、reasoning）

---

## 系统分层架构

```
┌─────────────────────────────────────────────────────────────┐
│ L3: Agent 语义适配层 (Semantic / LLM Layer)                 │
│  - Prompt Token 压缩与提取                                  │
│  - LLM 上下文语义纠错自愈 (In-Context Error Correction)     │
│  - MCP Server: send_covert_msg / listen_covert_msg         │
├─────────────────────────────────────────────────────────────┤
│ L2: 传输控制层 (Transport / Protocol Layer)                 │
│  - 自动阈值校准 (Otsu's 双峰分布判定)                      │
│  - 时分复用时钟槽 (TDM Time Slot)                           │
│  - 前导码唤醒检测 (Preamble 10101011)                       │
│  - 曼彻斯特编码 (消除长连0/1时钟漂移)                       │
│  - CRC8 / Reed-Solomon 纠错码                               │
│  - 滑动窗口与 ARQ 重传机制                                  │
├─────────────────────────────────────────────────────────────┤
│ L1: 调制解调层 (Modulation / Demodulation)                 │
│  - 模式 A: Eviction-Set 单组 Prime+Probe                   │
│  - 模式 B: 全缓存 FSK 频域占用率调制 + Goertzel 算法解码     │
│  - 模式 C: 端口竞争 / DRAM Row Buffer / 总线拥塞           │
├─────────────────────────────────────────────────────────────┤
│ L0: 硬件抽象层 (HAL - `cachetalk-hal`)                      │
│  - 跨架构抽象: `trait MicroArchHal { ... }`                 │
│  ├── x86_64: rdtscp / clflushopt / SMT 端口竞争             │
│  ├── aarch64 Linux: DC CIVAC / CNTVCT_EL0 寄存器            │
│  ├── Apple Silicon: S²C (LL/SC 竞争) / DMP 预取触发         │
│  └── RISC-V: Zicbom 扩展 / rdtime / rdcycle                │
└─────────────────────────────────────────────────────────────┘
```

---

## Rust Workspace 模块化设计

```
cachetalk/
├── Cargo.toml
├── crates/
│   ├── cachetalk-hal/          # 硬件抽象层
│   ├── cachetalk-phy/          # 物理层驱动
│   ├── cachetalk-modem/        # 调制解调与链路层
│   ├── cachetalk-calibrator/   # 自动化校准器
│   └── cachetalk-mcp/          # MCP Server 接入层
└── examples/
    ├── prime_probe_demo.rs
    ├── fsk_modem_demo.rs
    └── mcp_server_demo.rs
```

---

## 快速导航

- 详细技术论证与论文清单请查阅 [`docs/CACHE_SIDE_CHANNEL_LANDSCAPE.md`](docs/CACHE_SIDE_CHANNEL_LANDSCAPE.md)
- 系统架构设计请查阅 [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- 调研数据请查阅 [`artifacts/tmp/detailed-data/`](artifacts/tmp/detailed-data/)

---

## 当前状态

✅ **已完成**：
- 全芯片家族 Cache 微架构特性全景对照
- 核心攻击原语技术演进谱系
- 三大"零文件系统、零共享内存"隐蔽通信机制分析
- 抗检测体系与攻防对抗边界研究
- Rust 开源生态与参考代码库全景
- 系统分层架构设计（L0-L3）
- 详细调研数据归档
- Rust Workspace 初始化（5 个 crate，157 项单元/集成测试全部通过）
- HAL：x86_64 / aarch64 Linux / Apple Silicon 平台实现、Mock HAL 与 fallback
- 物理层：Prime+Probe、Cache Occupancy、Port Contention、DRAM Bank Probe
- 调制解调层：FSK + Goertzel、Manchester、帧封装、CRC8、Hamming(7,4)、TDM、ARQ
- 校准器：Otsu 阈值判定、驱逐集构建、延迟统计
- MCP Server：JSON-RPC stdio、`send_covert_msg` / `listen_covert_msg` / `get_channel_status`
- 可运行示例：`prime_probe_demo` / `fsk_modem_demo` / `mcp_server_demo`
- 跨 crate 集成测试：HAL 契约、调制解调全链路、物理层原语、校准器流程
- 性能基准：基于 std 计时的自定义 harness（modem / phy / calibrator / mcp）

🚧 **待实现**：
- RISC-V HAL（Zicbom / rdtime，目前由 fallback 占位）
- 真实硬件上的跨隔离端到端验证（当前 MCP 示例使用确定性模拟介质）
