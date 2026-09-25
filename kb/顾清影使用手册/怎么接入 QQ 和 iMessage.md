# 怎么接入 QQ 和 iMessage

接入之后可以在手机上和她聊，QQ 还能拉进群里。

## QQ（通过 NapCatQQ）

顾清影用 OneBot v11 协议接 QQ，主要适配 NapCatQQ。连接方式是反向 WebSocket：NapCat 作为客户端，连到顾清影的后台服务。

1. 装好并登录 NapCatQQ。
2. 在 NapCat 里添加一个反向 WebSocket 客户端，地址填 `ws://<运行顾清影的电脑>:8300/ws`（端口跟网页端一致，默认 8300），设置一个 token。
3. `gqy config` →「接入通讯平台」→「腾讯 QQ」：
   - 打开「启用」；
   - access_token 填和 NapCat 一样的 token；
   - 管理员里加上你自己的 QQ 号；
   - 私聊白名单里加上允许和她私聊的 QQ 号（建议至少加你自己）。
4. `gqy daemon status` 里「腾讯 QQ」一行显示已连接，就可以聊了。

QQ 用哪些模型，在同一个设置页的「配置模型」里单独设置，也可以直接继承全局设置。token 留空时只允许本机连接。

## iMessage（仅 macOS）

iMessage 桥接是项目仓库里的独立脚本，不在安装包里，需要先克隆项目仓库。

1. 在「信息」App 里登录一个给顾清影用的 Apple ID（可以和系统的 iCloud 账号不同）。
2. 运行 `scripts/imessage/install.sh`。第一次运行会生成 `~/.gqy/config/imessage.json`。
3. 编辑这个文件：在 `contacts` 里填允许和她私聊的联系人，把 `enabled` 改成 `true`。保存即生效，不用重启。
4. 授权：「系统设置 → 隐私与安全性 → 完全磁盘访问权限」，加入 `~/.local/bin/gqy-imessage`。

只回复白名单里的联系人。后台服务要在运行。安装后不要挪动 `scripts/imessage/` 目录，挪了桥接就会断，重装还要重新授权。

### 在 iMessage 里能做什么

- 收发文字、看你发来的照片；她会发表情包、图库里的图，也能把刚画好的图直接发给你。
- 你长按她的某条消息回复，她知道你在回哪一句；你给她点 ❤️ 之类的回应，她也看得到（点回应本身不会让她回一条）。她自己没法给你点回应，这是「信息」App 的限制。
- 在聊天里发下面这些指令，会马上得到回复，不会记进聊天内容：
  - `/new` 开一个新话题，旧话题都保留；`/topics` 看所有话题；`/topic 2` 切到第 2 个话题。
  - `/model` 看能用哪些模型；`/model 5` 或 `/model 模型名` 切换，只对这个聊天生效；`/model default` 恢复默认。
  - `/pause` 暂停回复，`/resume` 恢复。
  - `/help` 看指令说明。
- 群聊不支持，只和白名单里的人私聊。
