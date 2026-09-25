#!/usr/bin/env python3
"""派生三张 SVG。架构事实的源头是 gqy.structurizr.dsl 与 layers.dot，本脚本只负责画。

layers.svg    ← test_scripts/arch_dep_check.py 的 LAYERS + arch-dep-waivers.json（解析，不手抄）
boundary.svg  ← L1 边界：人 / gqy / 外部系统
processes.svg ← L2 进程与存储

渲染 PNG:  rsvg-convert -w 1500 layers.svg -o layers.png
"""
import json
import re
from pathlib import Path

ROOT = Path("/Users/mac/Projects/gqy-agent")
OUT = ROOT / "docs/architecture-model"
FONT = "PingFang SC, Hiragino Sans GB, Helvetica Neue, sans-serif"
MONO = "Menlo, monospace"
C = {"ipc": "#2E86C1", "sync": "#C0392B", "proc": "#116953", "data": "#512D81",
     "ext": "#7F8C8D", "person": "#083F77", "ink": "#1B2631", "mute": "#5D6D7E",
     "bad": "#922B21"}


def esc(t):
    return t.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def svg_open(w, h, title, sub):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" '
            f'viewBox="0 0 {w} {h}" font-family="{FONT}">'
            f'<defs><marker id="ar" viewBox="0 0 10 10" refX="8.5" refY="5" '
            f'markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse">'
            f'<path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/></marker></defs>'
            f'<rect width="{w}" height="{h}" fill="#fff"/>'
            f'<text x="26" y="34" font-size="19" font-weight="600" fill="{C["ink"]}">'
            f'{esc(title)}</text>'
            f'<text x="26" y="54" font-size="11.5" fill="{C["mute"]}">{esc(sub)}</text>')


def box(x, y, w, h, fill, stroke, text, sub=None, tc="#FFFFFF", mono=False, fs=12.5):
    fam = f' font-family="{MONO}"' if mono else ""
    s = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="{fill}" '
         f'stroke="{stroke}" stroke-width="1.2"/>']
    cx, cy = x + w / 2, y + h / 2
    if sub:
        s.append(f'<text x="{cx:.0f}" y="{cy - 2:.0f}" font-size="{fs}" font-weight="600" '
                 f'fill="{tc}" text-anchor="middle"{fam}>{esc(text)}</text>')
        s.append(f'<text x="{cx:.0f}" y="{cy + 15:.0f}" font-size="9.5" fill="{tc}" '
                 f'opacity="0.9" text-anchor="middle">{esc(sub)}</text>')
    else:
        s.append(f'<text x="{cx:.0f}" y="{cy + 4:.0f}" font-size="11.5" '
                 f'font-family="{MONO}" fill="{tc}" text-anchor="middle">{esc(text)}</text>')
    return "".join(s)


def label(x, y, t, color, anchor="middle", fs=9.5):
    return (f'<text x="{x:.0f}" y="{y:.0f}" font-size="{fs}" fill="{color}" '
            f'text-anchor="{anchor}" stroke="#fff" stroke-width="3.6" paint-order="stroke">'
            f'{esc(t)}</text>')


def line(pts, color, w=1.4, dash=None, arrow=True):
    d = "M" + " L".join(f"{a:.0f},{b:.0f}" for a, b in pts)
    da = f' stroke-dasharray="{dash}"' if dash else ""
    mk = ' marker-end="url(#ar)"' if arrow else ""
    return f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{w}" stroke-linejoin="round" stroke-linecap="round"{da}{mk}/>'


def curve(x1, y1, x2, y2, color, w=1.4):
    my = (y1 + y2) / 2
    return (f'<path d="M{x1:.0f},{y1:.0f} C{x1:.0f},{my:.0f} {x2:.0f},{my:.0f} '
            f'{x2:.0f},{y2:.0f}" fill="none" stroke="{color}" stroke-width="{w}" '
            f'stroke-opacity="0.8" marker-end="url(#ar)"/>')


def frame(x, y, w, h, color, text, ty=None):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="14" fill="none" '
            f'stroke="{color}" stroke-width="1.7" stroke-dasharray="8 5"/>'
            f'<text x="{x + 14}" y="{ty if ty is not None else y - 10}" font-size="12.5" '
            f'font-weight="700" fill="{color}">{esc(text)}</text>')


def frame_no_label(x, y, w, h, color):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="14" fill="none" '
            f'stroke="{color}" stroke-width="1.7" stroke-dasharray="8 5"/>')


def foot(y, lines, x=26):
    return "".join(f'<text x="{x}" y="{y + i * 19:.0f}" font-size="11" fill="{col}">'
                   f'{esc(t)}</text>' for i, (t, col) in enumerate(lines))


# ════════════════════════════ layers.svg ════════════════════════════
def render_layers():
    src = (ROOT / "test_scripts/arch_dep_check.py").read_text(encoding="utf-8")
    block = re.search(r"LAYERS = \[(.*?)\n\]", src, re.S).group(1)
    layers = [(n, re.findall(r'"([a-z_][a-z0-9_]*)"', b))
              for n, b in re.findall(r'\("(.+?)", \{(.*?)\}\)', block, re.S)]
    wa = json.loads((ROOT / "test_scripts/arch-dep-waivers.json")
                    .read_text(encoding="utf-8"))["waivers"]
    edges = sorted(((k.split("->")[0], k.split("->")[1], v["count"]) for k, v in wa.items()),
                   key=lambda e: -e[2])
    deg = {}
    for s, t, c in edges:
        deg[s] = deg.get(s, 0) + c
        deg[t] = deg.get(t, 0) + c

    W, left = 1500, 172
    bandh, vgap, top = 60, 62, 78
    bw = lambda n: max(74, 24 + 8.2 * len(n))
    pos, body = {}, []
    for i, (name, mods) in enumerate(layers):
        y = top + i * (bandh + vgap)
        spread = []
        for k, m in enumerate(sorted(mods, key=lambda m: -deg.get(m, 0))):
            spread.append(m) if k % 2 else spread.insert(0, m)
        widths = [bw(m) for m in spread]
        gap = min(16, (W - left - 30 - sum(widths)) / max(1, len(spread) - 1))
        body.append(f'<rect x="14" y="{y}" width="{W - 28}" height="{bandh}" rx="10" '
                    f'fill="#F8F9F9" stroke="#D5D8DC"/>')
        body.append(f'<text x="26" y="{y + bandh / 2 - 3:.0f}" font-size="12.5" '
                    f'font-weight="700" fill="{C["mute"]}">L{i}</text>')
        body.append(f'<text x="26" y="{y + bandh / 2 + 12:.0f}" font-size="10.5" '
                    f'fill="#797D7F">{esc(name)}</text>')
        x = left
        for m, w in zip(spread, widths):
            by = y + bandh / 2 - 15
            hot = deg.get(m, 0)
            body.append(box(x, by, w, 30, "#FDEDEC" if hot >= 8 else "#FFFFFF",
                            "#943126" if hot >= 8 else "#566573", m, tc=C["ink"]))
            pos[m] = (x + w / 2, by, by + 30)
            x += w + gap

    seen = {}
    for s, t, c in edges:
        if s not in pos or t not in pos:
            continue
        n = seen.get(s, 0)
        seen[s] = n + 1
        jx = ((n % 4) - 1.5) * 11
        body.append(curve(pos[s][0] + jx, pos[s][2], pos[t][0] + jx, pos[t][1],
                          C["sync"], w=0.9 + min(2.9, c * 0.17)))
        body.append(label(pos[t][0] + jx, pos[t][1] - 5, str(c), C["bad"], fs=10))

    h = top + len(layers) * (bandh + vgap) + 62
    out = svg_open(W, h, "gqy 模块层序 + 现存反向依赖",
                   "解析自 test_scripts/arch_dep_check.py 的 LAYERS 与 arch-dep-waivers.json"
                   f" · {sum(len(m) for _, m in layers)} 个模块 · {len(edges)} 条边 · "
                   f"{sum(e[2] for e in edges)} 处引用 · 2026-09-26")
    out += "".join(body) + foot(h - 38, [
        ("红边 = 门禁白名单放行的反向依赖（低层 → 高层）。数字 = 该边的代码引用处数，线越粗越多；"
         "淡红底的模块是引用数 ≥8 的热点。", C["bad"]),
        ("刻意不画约 190 条合规下行边，否则不可读。逐条边的文件与行号见 gqy.evidence.md。", C["mute"]),
    ]) + "</svg>"
    (OUT / "layers.svg").write_text(out, encoding="utf-8")
    return len(edges), sum(e[2] for e in edges), sum(len(m) for _, m in layers)


# ══════════════════════════ boundary.svg ══════════════════════════
PEOPLE = [("属主 / 管理员", "Owner · 默认不套沙盒"),
          ("成员账号", "Member · 回合套 Landlock"),
          ("聊天对面的第三方", "External · 正文不可信")]
PERSON_REL = ["终端 REPL · gqy ask · shellhook · 两个 TUI",
              "浏览器 WebUI :8300（默认绑 0.0.0.0）",
              "经 OneBot / iMessage 间接进来"]
EXTS = [("OneBot v11 端", "QQ 协议实现"), ("iMessage 桥", "Python LaunchAgent"),
        ("上游模型供应商", "OpenAI 兼容 · Anthropic · 中转"), ("MCP 服务器", "stdio 子进程"),
        ("播报供应商", "MiniMax · 小米 MiMo"), ("公网", "search / fetch / AUR / GitHub"),
        ("本机工具链", "rg chafa sh git pacman gh")]
EXT_REL = ["反向 WebSocket 连入 /ws", "gqy ask --session imessage-*", "流式补全 + 工具调用",
           "tools/list 与调用", "合成播报音频", "搜索与取页", "起子进程干活"]


def render_boundary():
    W, H = 1500, 620
    gx, gy, gw, gh = 545, 96, 400, 434
    b = [frame(gx, gy, gw, gh, "#0F635F", "软件系统 顾清影 gqy", ty=gy - 12),
         f'<text x="{gx + 16}" y="{gy + 26}" font-size="11.5" fill="#145A32">'
         f'一个 Rust crate · autobins=false · v0.7.0</text>']
    rows = [("gqy CLI", "入口与一次性客户端"), ("gqy __daemon", "唯一常驻 · 唯一跑回合"),
            ("gqy-voice", "可选 · --features voice"), ("3 个自重生 worker", "renderer / embedding / alarm"),
            ("7 个 store", "conversation · memory · kb · ledger …")]
    for i, (t, s) in enumerate(rows):
        b.append(box(gx + 26, gy + 44 + i * 76, gw - 52, 62, "#116953", "#0B4B3B", t, s, fs=13))
    b.append(f'<text x="{gx + gw / 2:.0f}" y="{gy + gh + 24}" font-size="10.5" '
             f'fill="{C["mute"]}" text-anchor="middle">内部细节见 processes.svg</text>')

    b.append(f'<text x="30" y="122" font-size="12" font-weight="700" fill="{C["person"]}">'
             f'人（3 类信任级）</text>')
    for i, (t, s) in enumerate(PEOPLE):
        y = 140 + i * 128
        b.append(box(30, y, 240, 74, C["person"], "#04213E", t, s, fs=13))
        y1, y2 = y + 37, gy + 150 + i * 62
        b.append(curve(270, y1, gx, y2, C["person"], w=1.8))
        b.append(label(400, (y1 + y2) / 2 - 14, PERSON_REL[i], C["person"], fs=9))

    b.append(f'<text x="1230" y="88" font-size="12" font-weight="700" fill="{C["ext"]}">'
             f'外部系统（7 类）</text>')
    step = (gh - 60) / (len(EXTS) - 1)
    for i, (t, s) in enumerate(EXTS):
        y = 100 + i * 62
        b.append(box(1230, y, 240, 52, "#7F8C8D", "#5D6D7E", t, s, fs=12))
        y0 = gy + 34 + i * step
        b.append(line([(gx + gw, y0), (1160, y0), (1160, y + 26), (1230, y + 26)], C["ext"], w=1.5))
        b.append(label((gx + gw + 1160) / 2, y0 - 6, EXT_REL[i], C["ext"], fs=9))

    b.append(foot(H - 40, [
        ("信任解析顺带产出 principal：blake3(入口, 账号 id, 用户 id) 取 24 hex，随会话冻结；"
         "记忆隔离、用量归属、沙盒根都从它派生。", C["mute"]),
        ("未验证：成员经中转线用 claude/codex/agy 时读得到该 CLI 自己的配置目录（含登录态）"
         "—— 代码自陈的固有取舍，未实测。", C["bad"]),
    ]))
    (OUT / "boundary.svg").write_text(
        svg_open(W, H, "顾清影 gqy — 边界（L1 System Context）",
                 "派生自 gqy.structurizr.dsl 的 L1-SystemContext 视图 · 2026-09-26 · commit 0041621b")
        + "".join(b) + "</svg>", encoding="utf-8")


# ══════════════════════════ processes.svg ══════════════════════════
STORES = [("conversation.db", "每身份 · 37 版迁移"), ("memory.db", "每 persona 一份"),
          ("evicted_context.db", "逐出回合归档"), ("ledger.db", "记账自有版本"),
          ("kb_meta + semantic", "机器级"), ("state/ cache/", "JSON · JSONL"),
          ("~/.gqy 目录树", "persona · extensions")]
WORKERS = [("gqy __renderer-worker", "长图 Markdown→PNG · 空闲 10min"),
           ("gqy __embedding-worker", "ONNX 外置 · 空闲 600s"),
           ("gqy __alarm-worker", "detached · 活得比 gqy 长")]


def render_processes():
    W, H = 1500, 664
    bx, by, bw_, bh = 20, 74, 1460, 512
    b = [f'<text x="{bx + bw_}" y="{by - 10}" font-size="12.5" font-weight="700" fill="#0F635F" text-anchor="end">软件系统 顾清影 gqy — 六个进程形态 + 七个 store（除 gqy-voice 外同一个二进制）</text>',
        frame_no_label(bx, by, bw_, bh, "#0F635F")]
    b.append(box(60, 100, 250, 82, "#2E86C1", "#1B4F72", "gqy CLI",
                 "REPL · ask · shellhook · stdio · TUI", fs=14))
    b.append(box(430, 92, 300, 98, C["person"], "#04213E", "gqy __daemon",
                 "唯一跑回合 · 0.0.0.0:8300 · core.sock", fs=15))
    b.append(box(830, 100, 230, 82, "#7D6608", "#5B4A06", "gqy-voice",
                 "--features voice 才构建", fs=14))
    b.append(line([(310, 141), (430, 141)], C["ipc"], w=2.4))
    b.append(label(370, 132, "core.sock", C["ipc"]))
    b.append(label(370, 156, "一连接一回合", C["ipc"], fs=8.5))
    b.append(line([(730, 132), (830, 132)], C["proc"], w=1.6))
    b.append(label(780, 125, "spawn", C["proc"], fs=8.5))
    b.append(line([(830, 152), (730, 152)], C["ipc"], w=1.6))
    b.append(label(780, 168, "VoiceAttach", C["ipc"], fs=8.5))

    for i, (t, s) in enumerate(WORKERS):
        x = 60 + i * 250
        b.append(box(x, 232, 230, 66, "#116953", "#0B4B3B", t, s, fs=11))
        b.append(line([(600, 190), (600, 212), (x + 115, 212), (x + 115, 232)], C["proc"]))
    b.append(label(300, 206, "daemon spawn 三个自重生 worker（env 变量 + argv[1] 双条件触发）",
                   C["proc"], anchor="start", fs=9.5))
    b.append(f'<text x="860" y="264" font-size="10.5" fill="{C["mute"]}">RLIMIT_AS 只在 Linux 施加；'
             f'macOS 上 setrlimit 回 EINVAL 故不设。</text>')
    b.append(f'<text x="860" y="284" font-size="10.5" fill="{C["mute"]}">daemon 另起外部进程：rg · chafa · sh · '
             f'git · pacman · gh · notify-send ·</text>')
    b.append(f'<text x="860" y="302" font-size="10.5" fill="{C["mute"]}">中转线 CLI（claude/codex/agy）· MCP 服务器；'
             f'成员回合下这些子进程继承同一套 Landlock 规则。</text>')

    sw, gp = 196, 8
    xs = [40 + i * (sw + gp) for i in range(len(STORES))]
    bus = 372
    b.append(line([(545, 190), (545, bus)], C["data"], w=2.0, arrow=False))
    b.append(label(556, 336, "StoreRegistry 按 principal 路由", C["data"], anchor="start", fs=9.5))
    b.append(line([(xs[0] + sw / 2, bus), (xs[-1] + sw / 2, bus)], C["data"], w=2.0, arrow=False))
    for x, (t, s) in zip(xs, STORES):
        b.append(line([(x + sw / 2, bus), (x + sw / 2, 424)], C["data"]))
        b.append(box(x, 424, sw, 72, "#512D81", "#3B1E5E", t, s, fs=10.5))
    b.append(f'<text x="40" y="528" font-size="10.5" fill="{C["mute"]}">紫线 = daemon 读写。'
             f'conversation.db 每身份一份、memory.db 每 persona 一份、kb 机器级不按 persona 切。</text>')
    b.append(f'<text x="40" y="548" font-size="10.5" fill="{C["mute"]}">不在文件里的一类：提示词、'
             f'工具描述 JSON、web/ 静态资源、jieba 与 o200k 词表 —— build.rs 编译期进二进制，改完必须重新构建。</text>')
    b.append(f'<text x="40" y="568" font-size="10.5" fill="{C["mute"]}">没有 pid 文件：daemon 存活靠 '
             f'IPC Ping + /proc/&lt;pid&gt;/{{stat,comm}}；端口 8300 被占则退到临时端口。</text>')

    b.append(foot(H - 26, [
        ("蓝=IPC · 绿=起子进程 · 紫=读写数据 · 深蓝=常驻。虚线框是软件系统 gqy 的范围。", C["mute"]),
        ("未验证：子代理不经 actor 直接 start_turn（src/tools/subagent.rs:999,1123），跳过了哪些回合闩锁待查。", C["bad"]),
    ]))
    (OUT / "processes.svg").write_text(
        svg_open(W, H, "顾清影 gqy — 进程形态与数据归属（L2 Containers）",
                 "派生自 gqy.structurizr.dsl 的 L2-Containers 视图 · 2026-09-26 · v0.7.0 / 0041621b")
        + "".join(b) + "</svg>", encoding="utf-8")


if __name__ == "__main__":
    print("layers (edges, refs, modules):", render_layers())
    render_boundary()
    render_processes()
