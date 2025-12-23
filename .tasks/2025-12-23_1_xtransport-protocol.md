# Context
File name: 2025-12-23_1
Created at: 2025-12-23_13:00:00
Created by: debin
Main branch: main
Task Branch: task/xtransport-protocol_2025-12-23_1
Yolo Mode: Off

# Task Description
在 xtransport 中设计一个通讯协议，使用类似于 KCP 的协议，但不要叫这个名字，然后在 client 和 server 中使用该协议发送 100M 的数据，并且测试一下传输速度

# Project Overview
- 传输层基础：支持多种传输协议，只要实现了 Read 和 Write trait 的传输都支持
- 协议名称：xtransport
- 基本功能：需要基本的序列号、校验和验证
- 测试场景：使用 TCP socket
- 数据传输：双向传输，暂时不需要确认机制
- 协议特性：需要实现分包/组包逻辑
- 内存分配：使用 no_std + alloc（堆上分配）

⚠️ WARNING: NEVER MODIFY THIS SECTION ⚠️
RIPER-5 核心规则：
- 每个模式必须声明 [MODE: MODE_NAME]
- RESEARCH: 仅收集信息，禁止建议
- INNOVATE: 仅探讨方案，禁止实施
- PLAN: 制定详细规格，禁止编码
- EXECUTE: 100%遵循计划实施
- REVIEW: 验证实施与计划一致性
⚠️ WARNING: NEVER MODIFY THIS SECTION ⚠️

# Analysis
- 项目为 Rust 工作空间，包含 xtransport（库）、client、server 三个子项目
- 所有源代码文件尚未创建，需要从零开始实现
- xtransport 设计为 no_std 兼容的可靠传输协议
- 需要定义自定义 Read/Write trait，兼容标准库和 no_std
- 协议包头：16字节（魔数4 + 版本1 + 标志1 + 序号4 + 长度2 + CRC32 4）
- 分包大小：约 64KB（减去包头后的有效载荷）
- 使用 crc32fast 进行校验和计算

# Proposed Solution
采用流式传输层方案（方案二）：
- XTransport<T: Read + Write> 结构体包装底层传输
- 实现自定义 Read/Write trait，在 std feature 下提供标准库互操作
- 内部使用 Vec<u8> 在堆上分配缓冲区（64KB）
- 自动处理分包和组包逻辑
- 每个包带序列号和 CRC32 校验
- Client 和 Server 通过 TCP 双向传输 100MB 数据并测量速度

# Current execution step: "1. 创建任务文件和功能分支"

# Task Progress

# Final Review:
