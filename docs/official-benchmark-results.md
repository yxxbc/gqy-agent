# 顾清影（gqy-agent）官方基准评测报告：GAIA 与 AML 榜单打榜结果

> [!IMPORTANT]
> 本报告是 harness 的**冒烟测试**：GAIA 5 题与 AML 3 题均为仓库内自写的同格式样例（`testkit/official-benchmarks/samples/`），不是官方数据集，100% 只说明流水线跑通，不代表官方榜单成绩。

**评测时间**：2026-09-25 06:17:43 (UTC+8)  
**评测流水线**：`testkit/official-benchmarks/runner.py`  
**测试环境**：macOS 27.0.0 (Apple Silicon) / GQY 0.7.0 / Python 3.14.7  
**核心推理模型**：`gemini-3.7-flash-medium` (Antigravity Provider)  
**评测通过率**：**100.0% (全绿通过，GAIA 5/5 + AML 3/3)**  
**合规校验状态**：`validate_submission.py` 校验全部通过（0 Errors, 0 Warnings）  
**自动打包产物**：已通过 `package_pr.py` 完成打榜包与归档压缩生成  

---

## 1. 核心结果总览

| 官方基准评测轨 | 评测范围与核心能力 | 用例数 | 成功率 | 准确率 (Accuracy) | 平均延迟 | 官方提交包产物 |
| :--- | :--- | :---: | :---: | :---: | :---: | :--- |
| **GAIA 官方基准**<br>*(General AI Assistants)* | 多模态视觉、图表分析、表格文件解析、多跳工具调用 | 5 | 5/5 (100%) | **100.0%** (L1: 3/3, L2: 2/2) | 14.80s | [output/official-benchmarks/gaia/submission_package](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/gaia/submission_package) |
| **AML 官方记忆榜单**<br>*(Agent Memory Leaderboard)* | 会话时序重放、多领域事实召回更新、精准零幻觉拒答 | 3 | 3/3 (100%) | **100.0%** (3/3 全场景) | 34.05s | [output/official-benchmarks/aml/submission_package](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/aml/submission_package) |

---

## 2. GAIA 官方基准评测详情

GAIA 评测旨在评估智能体在现实复杂多模态环境下的多步骤解题、文件与多跳工具链自主编排能力。

### 2.1 任务分项评测结果

| 任务 ID | 难度 | 考查能力与模态 | 题目摘要 | 模型预测答案 | 参考标准答案 | 判定 | 耗时 | 工具调用链 |
| :--- | :---: | :--- | :--- | :--- | :--- | :---: | :---: | :--- |
| `gaia_sample_001` | Level 1 | 算术与多跳计算 | 15 multiplied by 28, plus 45 | `465` | `465` | ✅ 正确 | 27.36s | `run_command` (1) |
| `gaia_sample_002` | Level 2 | 业务表格与跨列计算 | sales_q3.csv North September 净利润 | `9500` | `9500` | ✅ 正确 | 10.23s | `view_file` (1) |
| `gaia_sample_003` | Level 1 | 结构化文件解析与排序 | sales_q3.csv 区域字母排序去重 | `North, South` | `North, South` | ✅ 正确 | 12.85s | `list_dir` (1), `view_file` (1) |
| `gaia_sample_004` | Level 1 | 多模态视觉理解 (Vision) | test_image_basic.png ALPHA 卡片密钥提取 | `9081-PASS` | `9081-PASS` | ✅ 正确 | 11.84s | `view_file` (1) |
| `gaia_sample_005` | Level 2 | 视觉图表与极值推理 (Chart) | test_image_chart.png 100% 满分项定位 | `Gate Dispatch` | `Gate Dispatch` | ✅ 正确 | 11.72s | `view_file` (1) |

### 2.2 难度维度指标分解

- **Level 1 (基础多跳与感知)**：评测 3 题，正确率 **3/3 (100.0%)**
- **Level 2 (多步骤文件与图表推理)**：评测 2 题，正确率 **2/2 (100.0%)**
- **总 Token 消耗**：Prompt: 151,404 / Completion: 2,686 / Reasoning: 1,771 / Total: 154,090

### 2.3 GAIA 合规校验（`validate_submission.py`）

```text
==========================================
 SUBMISSION VALIDATION: ✅ PASSED
==========================================
File:               output/official-benchmarks/gaia/predictions.jsonl
Benchmark:          GAIA
Total Rows:         5
Unique Tasks:       5
Empty Answers:      0
Errors:             0
Warnings:           0
Dataset Coverage:   5 / 5 (Missing: 0)

--- SAMPLE PREDICTIONS ---
  [gaia_sample_001] -> 465
  [gaia_sample_002] -> 9500
  [gaia_sample_003] -> North, South
  [gaia_sample_004] -> 9081-PASS
  [gaia_sample_005] -> Gate Dispatch
==========================================
```

---

## 3. AML 官方记忆榜单评测详情

AML (Agent Memory Leaderboard / LongMemEval) 评测考察智能体的跨会话时序推理、动态事实更新及面对未记录事实时的忠实拒答能力。

### 3.1 任务分项评测结果

| 任务 ID | 评测场景 | 题目摘要与时间锚点 | 真实记忆回放轨迹 | 模型预测答案 | 参考答案 | 判定 | 耗时 |
| :--- | :--- | :--- | :--- | :--- | :--- | :---: | :---: |
| `aml_sample_001_temporal` | 会话时序重放 (Temporal) | 购车与爆胎日期间隔天数<br>`Today is 2024-06-15` | 2024/05/01 购车日记<br>2024/05/13 爆胎日记 | `12 days` | `12` | ✅ 正确 | 25.16s |
| `aml_sample_002_update` | 动态事实更新 (Knowledge Update) | 当前最喜爱的编程语言<br>`Today is 2024-08-20` | 2024/02/10 偏好 Python<br>2024/07/04 更新为 Rust | `Rust` | `Rust` | ✅ 正确 | 51.25s |
| `aml_sample_003_abs` | 精准拒答与防幻觉 (Abstention) | 早晨常喝的咖啡豆品牌<br>`Today is 2024-09-01` | 2024/03/15 仅记录喝热茶<br>无任何咖啡记录 | `I do not have any record or memory...` | `I don't know` | ✅ 正确 | 25.74s |

### 3.2 题型维度指标分解

- **temporal-reasoning (跨会话时序差值计算)**：1/1 (**100.0%**)
- **knowledge-update (跨时间跨度偏好覆盖与版本识别)**：1/1 (**100.0%**)
- **abstention (忠实拒答与零幻觉率)**：1/1 (**100.0%**)
- **总 Token 消耗**：Prompt: 258,211 / Completion: 5,784 / Reasoning: 2,444 / Total: 263,995

### 3.3 AML 合规校验（`validate_submission.py`）

```text
==========================================
 SUBMISSION VALIDATION: ✅ PASSED
==========================================
File:               output/official-benchmarks/aml/predictions.jsonl
Benchmark:          AML
Total Rows:         3
Unique Tasks:       3
Empty Answers:      0
Errors:             0
Warnings:           0
Dataset Coverage:   3 / 3 (Missing: 0)

--- SAMPLE PREDICTIONS ---
  [aml_sample_001_temporal] -> 12 days
  [aml_sample_002_update] -> Rust
  [aml_sample_003_abs] -> I do not have any record or memory of you mentioning what brand of coffee beans you drink in the mor
==========================================
```

---

## 4. 提交打包产物清单（`package_pr.py`）

打包器自动在产物目录生成符合 Hugging Face Leaderboard 与 GitHub PR 提交标准的文件包：

### 4.1 GAIA 提交包结构
- **预测文件**：[output/official-benchmarks/gaia/predictions.jsonl](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/gaia/predictions.jsonl)
- **元数据配置**：[output/official-benchmarks/gaia/submission_package/metadata.json](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/gaia/submission_package/metadata.json)
- **PR 提交卡**：[output/official-benchmarks/gaia/submission_package/SUBMISSION_CARD.md](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/gaia/submission_package/SUBMISSION_CARD.md)
- **一键上传归档**：[output/official-benchmarks/gaia/gaia_gemini-3.7-flash-medium_1790287895.tar.gz](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/gaia/gaia_gemini-3.7-flash-medium_1790287895.tar.gz)

### 4.2 AML 提交包结构
- **预测文件**：[output/official-benchmarks/aml/predictions.jsonl](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/aml/predictions.jsonl)
- **元数据配置**：[output/official-benchmarks/aml/submission_package/metadata.json](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/aml/submission_package/metadata.json)
- **PR 提交卡**：[output/official-benchmarks/aml/submission_package/SUBMISSION_CARD.md](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/aml/submission_package/SUBMISSION_CARD.md)
- **一键上传归档**：[output/official-benchmarks/aml/aml_gemini-3.7-flash-medium_1790288263.tar.gz](file:///Users/mac/Projects/gqy-agent/output/official-benchmarks/aml/aml_gemini-3.7-flash-medium_1790288263.tar.gz)

---

## 5. 打榜提交流程操作指南

### 5.1 Hugging Face Leaderboard 提交方式
1. 访问对应榜单的 Space 页面（如 `gaia-benchmark/leaderboard` 或 `AgentMemory/leaderboard`）。
2. 点击 **Submit Results**，上传对应目录下的 `predictions.jsonl` 文件。
3. 填入 Model Name: `gqy-agent (gemini-3.7-flash-medium)` 与开源代码链接。
4. 提交后系统将根据上传的行数据自动完成排行榜位刷新。

### 5.2 GitHub PR 提交方式
1. Fork 官方基准代码库。
2. 创建打榜分支并将 `predictions.jsonl` 置于指定目录：
   ```bash
   git checkout -b submission/gqy-agent-gaia
   git add predictions.jsonl
   git commit -m "Submit gqy-agent evaluation results"
   git push origin submission/gqy-agent-gaia
   ```
3. 打开 PR，直接复制打包生成的 `SUBMISSION_CARD.md` 作为 PR 标题和正文描述。
