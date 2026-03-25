
CEX SPOT ARBITRAGE SYSTEM
​
交易所现货套利系统
开发设计文档

| 版本： | v1.0.0 |
| --- | --- |
| 日期： | 2026-03-24 |
| 作者： | Johnny |
| 密级： | 机密 / Confidential |

# 一、项目概述 Project Overview
## 项目背景
本项目旨在构建一套低延迟、高可靠的中心化交易所（CEX）现货跨所套利系统，通过实时监控多个交易所的价格差异，在扣除手续费、汇率转换等成本后，发现并执行套利交易。前期以模拟交易为主，不进行实际下单，仅计算并展示套利机会和预估利润。
## 目标交易所

| 交易所 | 报价币种 | Taker费率 | API类型 | 服务器位置 |
| --- | --- | --- | --- | --- |
| Binance | USDT | 0.10% | WebSocket + REST | 东京 / 新加坡 |
| Bybit | USDT | 0.10% | WebSocket + REST | 东京 / 新加坡 |
| Upbit | KRW | 0.25% | WebSocket + REST | 首尔 (AWS ap-ne-2) |
| Bithumb | KRW | 0.25% | WebSocket + REST | 首尔 |

## 目标交易对
报价代币：USDT（统一折算基准）
交易币种：BTC、ETH、SOL、XRP
交易对组合：4个交易所 × 4个币种 = 16个交易对，共 4×3 = 12个跨所套利方向/币种，总计 48个套利路径

| 重要说明 前期开发阶段仅进行模拟交易，不会实际下单。系统将通过算法计算发现盈利机会后，展示“买入 X 数量可盈利 Y 美金”的信号，供人工审核和策略验证。 |
| --- |

# 二、系统架构 System Architecture
## 整体分层架构
系统采用五层分层架构，从上到下依次为：

| 层级 | 名称 | 职责 | 技术栈 |
| --- | --- | --- | --- |
| L1 | 行情数据层 | WebSocket连接管理、行情接收、Orderbook维护 | Rust/tokio-tungstenite, Lock-free Ring Buffer |
| L2 | 数据规范化层 | 汇率转换、价格统一到USD等价、时钟同步 | KRW/USD实时汇率、NTP Offset |
| L3 | 策略引擎层 | 价差计算、手续费扣除、滑点估算、信号生成 | Single-thread hot loop, CPU-pinned |
| L4 | 风控与仓位层 | 仓位管理、资金再平衡、Kill Switch | Position Manager, Balance Rebalancer |
| L5 | 基础设施层 | 监控、日志、告警、配置热更新 | Prometheus/Grafana, Async Logging |

## 数据流转流程
核心数据流转路径如下：
- 各交易所 WebSocket 推送行情更新（Best Bid/Ask + Depth）
- 行情解析器将原始数据解析为统一的内部 Orderbook 格式（~50μs）
- KRW 报价的交易所通过实时汇率转换为 USD 等价
- **过时行情检测**：每条行情附带本地接收时间戳，超过阈值（默认 3 秒）未更新的交易所数据标记为 stale，策略引擎跳过该交易所的所有套利计算
- 策略引擎扫描 4×4 价差矩阵，识别套利机会（~5μs）
- 信号生成器输出套利信号：买入交易所 / 卖出交易所 / 数量 / 预估利润
- 前端 Dashboard 实时展示套利机会和历史信号

## WebSocket 连接管理
各交易所 WebSocket 连接需具备以下容错机制：
- **心跳检测**：定期发送 ping，超过 10 秒无 pong 则判定连接失效
- **自动重连**：断连后指数退避重连（1s, 2s, 4s, 8s...最大 30s），重连成功后通过 REST API 拉取全量 Orderbook 快照
- **数据完整性校验**：Bybit/Bithumb 的增量更新需校验序列号连续性，缺失则丢弃并重建快照
- **连接状态上报**：各连接的状态（connected/reconnecting/stale）实时推送至 Dashboard 和 Prometheus 指标

## 技术栈选择

| 模块 | 技术选型 | 选型理由 |
| --- | --- | --- |
| 核心引擎 | Rust + Tokio | 零GC停顿、内存安全、async生态成熟、接近C++性能 |
| WebSocket客户端 | tokio-tungstenite | Rust原生异步WebSocket，与tokio完美集成 |
| JSON解析 | serde_json + simd-json | Zero-copy解析，SIMD加速，减少内存分配 |
| 前端面板 | React + TypeScript | 组件化架构，实时数据可视化，复用现有技术栈 |
| 实时通信 | WebSocket (ws库) | 服务端向前端推送实时价格和套利信号 |
| 数据存储 | ClickHouse + Redis | ClickHouse存储历史tick数据，Redis缓存实时状态 |
| 监控告警 | Prometheus + Grafana | 延迟指标、信号质量、系统健康监控 |
| 监控脚本 | Python + Pandas | 策略参数分析、异常检测、运维辅助（离线） |

# 三、汇率处理方案 Exchange Rate Handling
跨韩国交易所套利的核心难点在于汇率转换。Upbit和Bithumb使用KRW报价，而Binance和Bybit使用USDT报价，因此必须引入实时汇率来统一计算。
## 汇率来源优先级

| P | 汇率源 | 说明 | 更新频率 |
| --- | --- | --- | --- |
| 1 | USDT/KRW OTC实时价 | 最接近真实套利场景的汇率 | 实时 |
| 2 | 交易所隐含汇率 | 通过 BTC的KRW价和USDT价反算 | 随行情更新 |
| 3 | ECB/银行中间价 | 备用源，更新较慢 | 每小时/每日 |

## 汇率计算公式
隐含汇率推算方法：
KRW_USD_rate = Upbit_BTC_KRW_mid / Binance_BTC_USDT_mid

统一价格计算：
对于KRW报价交易所：Price_USD = Price_KRW / KRW_USD_rate
对于USDT报价交易所：Price_USD = Price_USDT（假定 USDT ≈ USD）

## 汇率使用规则（避免循环依赖）

当使用隐含汇率（通过 BTC 价格反算 KRW/USD）时，存在逻辑循环：用 BTC 计算出的汇率再去判断 BTC 本身的跨所价差，会导致 BTC 的价差被汇率计算抵消。因此必须遵循以下规则：

**模式一：独立汇率源（推荐）**
使用 USDT/KRW OTC 实时价作为汇率源时，所有币种（含 BTC）的跨所套利信号均有效，不存在循环依赖。

**模式二：隐含汇率（备用）**
当 OTC 汇率源不可用时，使用 BTC 隐含汇率作为备用：
- BTC 本身的跨所价差信号**必须标记为无效**（因为汇率由 BTC 自身推算）
- 仅 ETH、SOL、XRP 的跨所套利信号有效（交叉套利）
- 系统应在 Dashboard 上明确标注当前使用的汇率模式

**汇率健康检查**：
- 隐含汇率与 OTC 汇率偏差 > 0.5% 时触发告警
- 汇率源超过 30 秒未更新时，标记为 stale 并暂停依赖该汇率的信号生成

| 汇率风险提示 汇率源的选择直接影响套利判断的准确性。银行中间价、USDT/KRW OTC价、交易所隐含汇率三者之间本身就有价差，必须使用与实际资金流转渠道一致的汇率源。注意韩国泡菜溢价（Kimchi Premium）通常在1-5%之间波动。 |
| --- |

# 四、套利算法设计 Arbitrage Algorithm
## 价差计算模型
对于每个币种，系统维护一个 4×4 的价差矩阵（Spread Matrix），行为买入交易所、列为卖出交易所。

每格计算公式：

Gross_Spread = (Sell_Bid_USD - Buy_Ask_USD) / Buy_Ask_USD × 100%
Total_Fee = Buy_Fee + Sell_Fee
Net_Spread = Gross_Spread - Total_Fee

Net_Spread > 0 时产生套利信号。
## 利润计算模型
当发现正价差时，计算具体套利数量和利润：

Max_Qty = min(Trade_Amount_USD / Buy_Ask_USD, Buy_AskQty, Sell_BidQty)
Buy_Cost = Max_Qty × Buy_Ask_USD × (1 + Buy_Fee)
Sell_Revenue = Max_Qty × Sell_Bid_USD × (1 - Sell_Fee)
Profit_USD = Sell_Revenue - Buy_Cost

示例：假设 BTC 在 Binance Ask = $87,200，Upbit Bid = ₩128,800,000（汇率 1,450，等价 $88,827）
- Gross Spread = (88,827 - 87,200) / 87,200 = 1.865%
- Total Fee = 0.10% + 0.25% = 0.35%
- Net Spread = 1.865% - 0.35% = +1.515%
- 交易量 $10,000 → 买入 0.1147 BTC → 预估利润 +$151.50

## 滑点估算模型
实际交易中，大额订单会“吃掉”多个价格档位，产生滑点。滑点模型如下：
- Level 1（简化）：仅使用 Best Bid/Ask，忽略深度——适用于小额套利
- Level 2（推荐）：基于 Orderbook 深度计算加权均价（VWAP），更接近真实成交价
- Level 3（高级）：历史滑点统计回归模型，根据订单量和市场波动率动态调整

# 五、交易所API接入方案 Exchange API Integration
## Binance

| 项目 | 详情 |
| --- | --- |
| WebSocket端点 | wss://stream.binance.com:9443/ws/<symbol>@bookTicker |
| 订阅格式 | {"method":"SUBSCRIBE","params":["btcusdt@bookTicker"],"id":1} |
| 响应字段 | s(交易对), b(bestBid), B(bidQty), a(bestAsk), A(askQty) |
| 费率 | Maker 0.10% / Taker 0.10%（VIP0），BNB抵扣后 0.075% |
| 频率限制 | WebSocket无限制，REST 1200次/分钟 |

## Upbit

| 项目 | 详情 |
| --- | --- |
| WebSocket端点 | wss://api.upbit.com/websocket/v1 |
| 订阅格式 | [{"ticket":"arb"},{"type":"orderbook","codes":["KRW-BTC","KRW-ETH","KRW-SOL","KRW-XRP"]}] |
| 响应字段 | cd(代码), obu[](orderbook_units: ap/as/bp/bs) |
| 费率 | 0.25% (Taker)，可通过KRW充值优惠降低 |
| 频率限制 | REST: 秒 30次/分 900次，下单: 秒 8次/分 200次 |
| 特殊说明 | 服务器位于AWS首尔区域(ap-northeast-2)，报价单位KRW |

## Bithumb

| 项目 | 详情 |
| --- | --- |
| WebSocket端点 | wss://pubwss.bithumb.com/pub/ws |
| 订阅格式 | {"type":"orderbookdepth","symbols":["BTC_KRW","ETH_KRW","SOL_KRW","XRP_KRW"]} |
| 响应字段 | content.list[](symbol, orderType, price, quantity), datetime |
| 费率 | 0.25% (Taker)，兑换券可降至 0.04% |
| 频率限制 | REST: 秒 20次/分 1200次，下单: 秒 10次/分 150次 |
| 特殊说明 | 韩国本土 Bithumb（非 Bithumb Global），报价单位 KRW。Orderbook 为增量推送，需维护本地快照，断连时通过 REST API 重建全量 |

## Bybit

| 项目 | 详情 |
| --- | --- |
| WebSocket端点 | wss://stream.bybit.com/v5/public/spot |
| 订阅格式 | {"op":"subscribe","args":["orderbook.1.BTCUSDT"]} |
| 响应字段 | type(snapshot/delta), data.b[][](bids), data.a[][](asks), ts |
| 费率 | Maker 0.10% / Taker 0.10%（VIP0） |
| 特殊说明 | 支持snapshot+delta更新模式，3秒无变化会重发snapshot |

# 六、前端界面设计 Frontend Design
## Dashboard 核心模块

| 模块 | 功能描述 | 数据更新频率 |
| --- | --- | --- |
| 实时价格矩阵 | 4个交易所 × 4个币种的USD等价价格，带Sparkline走势图 | 实时（~500ms） |
| 套利机会表 | 按净价差排序，展示买卖方向、数量、预估利润 | 实时（~500ms） |
| 价差热力图 | 4×4 矩阵，颜色编码净价差正负 | 实时（~500ms） |
| 历史信号日志 | 近期发现的盈利信号列表，可筛选和导出 | 事件驱动 |
| 统计概览卡片 | 盈利信号数、总预估利润、平均价差、最佳币种 | 实时 |
| 汇率显示 | 实时KRW/USD汇率，注明数据源 | 实时 |

## UI设计规范
- 配色方案：深空蓝紫背景 (#08081a, #0e0f28)，绿色强调 (#22c55e)，青色辅助 (#06b6d4)
- 字体：JetBrains Mono / SF Mono 等宽字体，体现交易系统专业感
- 数据显示：绿色 = 盈利/买入方，红色 = 亏损/卖出方，青色 = 信息/中性
- 交互：支持币种筛选、交易量调整、点击信号查看详情

# 七、部署方案 Deployment
## 地域选择
部署地域是套利系统的胜负手。四个交易所的服务器分布决定了网络延迟格局：

| 部署方案 | 首尔节点 | 东京节点 | 适用场景 | 阶段 |
| --- | --- | --- | --- | --- |
| 方案B（Phase 1 推荐） | 所有服务集中部署 | — | 简化运维，韩所延迟最优，适合模拟阶段验证 | Phase 1-2 |
| 方案A（Phase 3+ 推荐） | 主节点：策略引擎 + Upbit/Bithumb行情 | 辅助节点：Binance/Bybit行情+下单 | 全量套利，最低综合延迟 | Phase 3+ |
| 方案C | — | 所有服务集中部署 | Binance/Bybit延迟最优 | 备选 |

注意：方案A双节点部署时，首尔-东京间 10-15ms 的节点间延迟意味着策略引擎收到 Binance/Bybit 行情比韩所行情晚约 10ms。在实盘阶段需评估该延迟对信号时效性的影响，必要时在东京节点部署独立的 Binance-Bybit 子引擎。Phase 1-2 模拟阶段建议使用方案B（首尔单节点），简化架构优先验证策略逻辑。

| 部署禁区 绝对不要部署在中国大陆。到韩国交易所的网络延迟、防火墙干扰、以及合规风险都会让系统完全丧失竞争力。 |
| --- |

## 网络延迟估算

| 路径 | 普通网络 | 专线 |
| --- | --- | --- |
| 首尔 → Upbit/Bithumb | < 1ms | < 0.5ms |
| 东京 → Binance/Bybit | 1-3ms | < 1ms |
| 首尔 → 东京（节点间） | 10-15ms | < 5ms |
| 中国大陆 → 首尔 | 50-200ms + 不稳定 | 不建议 |

# 八、开发阶段规划 Development Phases
## Phase 1：模拟监控系统（当前阶段）
目标：数据采集 + 套利信号发现 + 可视化展示
周期：2-3周
- 接入四个交易所的 WebSocket 实时行情
- 实现KRW/USD汇率转换服务
- 开发套利计算引擎（价差矩阵 + 利润计算）
- 构建 React Dashboard 前端面板
- 历史信号记录与统计分析

## Phase 2：策略优化与回测
目标：策略验证 + 参数调优
周期：2-3周
- 集成10天以上的实时tick数据
- 开发回测引擎，基于历史数据验证套利策略
- 引入滑点模型（L2 VWAP）
- 优化价差阈值、仓位大小、交易对权重
- 生成策略回测报告（胜率、Sharpe、最大回撤）

## Phase 3：实盘对接（小额）
目标：小额实盘验证
周期：3-4周
- 接入交易所下单API（先接Binance+一个韩所）
- 实现双边并行下单逻辑
- 开发风控模块（最大仓位、每日亏损限额、Kill Switch）
- 小额资金实盘跑通全流程

## Phase 4：全自动化运维
目标：自动化监控 + 全自动化运维
周期：3-4周
- 异常检测脚本（API延迟突增、成交率下降、价差异常时自动告警）
- 自动Rebalancing（各交易所间资金再平衡）
- 合规与风控报告自动生成
- 策略参数自动调优（基于历史数据统计分析）

# 九、风控与安全 Risk Management
## 交易风控规则

| 风控项 | 阈值 | 触发动作 |
| --- | --- | --- |
| 单笔最大仓位 | 可配置（默认 $10,000） | 超出时拒绝下单 |
| 单币种仓位上限 | 可配置（BTC/ETH: $10,000, SOL/XRP: $5,000） | 流动性较差的币种使用更低仓位上限 |
| 每日亏损限额 | 可配置（默认 $1,000） | 达到限额时停止所有交易 |
| 最小净价差阈值 | 可配置（默认 0.1%） | 低于阈值时不发信号 |
| 连续亏损熔断 | 可配置（默认连续 5 次信号回测亏损） | 暂停该币种/方向的信号生成 30 分钟 |
| API延迟监控 | 可配置（默认 > 500ms） | 延迟异常时暂停该交易所交易 |
| 汇率源异常 | 汇率超过 30s 未更新或偏差 > 1% | 暂停所有 KRW 相关套利，降级至备用汇率源 |
| Kill Switch | 手动 / 自动 | 紧急情况下撤销所有挂单、停止交易 |

## 其他风险考量
- 汇率风险：KRW/USD汇率波动可能吃掉套利利润，需监控汇率源健康度
- 提币风险：跨所套利需考虑提币时间和手续费（前期模拟阶段不涉及）
- 流动性风险：小币种/小交易所的深度不足可能导致大额滑点
- 合规风险：韩国对加密货币有资本管制，需确保合规操作
- API风险：交易所可能随时变更API格式或频率限制，需有降级机制
- 过时行情风险：WebSocket 断连或延迟导致的 stale data 可能产生虚假信号，必须在策略层过滤

# 十、项目目录结构 Project Structure
推荐的 Monorepo 目录结构如下：

| 路径 | 说明 |
| --- | --- |
| cex-arbitrage/ | 项目根目录 |
| ├── engine/ | Rust核心引擎 |
| │   ├── src/feeds/ | 各交易所WebSocket接入器 |
| │   ├── src/normalizer/ | 数据规范化 + 汇率转换 |
| │   ├── src/strategy/ | 套利策略引擎 |
| │   ├── src/risk/ | 风控模块 |
| │   └── src/ws_server/ | 向前端推送的WebSocket服务 |
| ├── dashboard/ | React + TypeScript前端 |
| │   ├── src/components/ | UI组件（PriceMatrix, SpreadHeatmap等） |
| │   └── src/hooks/ | WebSocket连接、数据状态管理 |
| ├── backtest/ | Python回测引擎 |
| ├── scripts/ | 运维脚本、参数分析工具 |
| ├── infra/ | Docker Compose, 监控配置 |
| └── docs/ | 文档 |

# 附录 Appendix
## A. 术语表

| 术语 | 说明 |
| --- | --- |
| Spread | 两个交易所之间的价差百分比 |
| Net Spread | 扣除双边手续费后的净价差 |
| Kimchi Premium | 韩国交易所相对国际交易所的价格溢价（泡菜溢价） |
| VWAP | Volume Weighted Average Price，成交量加权均价 |
| Slippage | 预期成交价与实际成交价的差异 |
| Hot Path | 行情接收→策略计算→下单执行的关键延迟路径 |
| Kill Switch | 紧急停止所有交易活动的安全机制 |
| Tick-to-Trade | 从行情更新到下单完成的端到端延迟 |
| Orderbook Depth | 订单簿深度，各价位的挂单量 |
| Rebalancing | 各交易所之间的资金和币种再平衡 |

## B. 参考文档
- Binance Spot API Documentation: https://binance-docs.github.io/apidocs/spot/en/
- Bybit V5 API Documentation: https://bybit-exchange.github.io/docs/v5/
- Upbit Open API: https://global-docs.upbit.com/
- Bithumb Pro API: https://github.com/bithumb-pro/bithumb.pro-official-api-docs
- ExchangeRate API (KRW/USD): https://exchangerate-api.com/

## C. 版本历史

| 版本 | 日期 | 作者 | 变更说明 |
| --- | --- | --- | --- |
| v1.0.0 | 2026-03-24 | Johnny | 初始版本，包含完整系统设计 |
