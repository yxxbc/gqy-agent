// 设置页字段模式表。
//
// 一切默认值/范围/枚举都抄自 Rust 侧(src/config/**),中文标签抄自 TUI
// 配置器(src/config_tui/**)的 t("English", "中文")。改 Rust 默认值时请同步。
//
// 字段描述符:
//   { key|path, label, hint?, kind, min?, max?, step?, unit?, integer?, choices?,
//     default, optional?, nullable?, hidden?, providerKey?, modelKey?, capability?,
//     showWhen? }
// kind ∈ toggle | number | text | textarea | select | secret | secret-list |
//        model-pool | model-ref | id-list | string-list | kv | rate-limit |
//        session-limits | identity-mappings | u32-list | json
//
// 本目录各段按文件名顺序拼成一份 /settings-schema.js,外面包一层 IIFE
// (src/web/asset_rules.rs)。各段只写顶层 const,拼进同一个函数作用域后互相可见。
// 这里放多个分段共用的常量。

const TOOL_SCOPE_CHOICES = [
  { value: "off", label: "关闭" },
  { value: "dev", label: "仅 dev 模式" },
  { value: "normal", label: "仅 normal 模式" },
  { value: "all", label: "全部模式" },
];

const REASONING_CHOICES = [
  { value: "summary", label: "摘要" },
  { value: "full", label: "完整" },
  { value: "hidden", label: "隐藏" },
];

const OVERFLOW_CHOICES = [
  { value: "compact", label: "摘要压缩" },
  { value: "pop", label: "丢弃最旧" },
];

const DEFAULT_MODERATION_KEYWORDS = [
  "3p", "4p", "64", ":(){ :|:& };:", "> /dev/sda", "FtM", "IEPL", "IPLC", "K粉",
  "LGBTQ", "MtF", "Netflix拼车", "OD", "Spotify车位", "V2board", "VPN",
  "chmod -R 777 /", "chown -R 777 /", "clash/config", "cnm", "dd if=/dev/zero",
  "dick", "hysteria://", "iCloud拼车", "lsp", "mkfs.ext4", "mkfs.xfs", "nmsl",
  "ntr", "rm -fr /*", "rm -rf /*", "sb", "ss://", "ssr://", "sub?target=", "suck",
  "trojan://", "tuic://", "vless://", "vmess://", "zzzq", "三年自然灾害", "东三省",
  "中美贸易", "主义", "京喜", "人肉", "人身攻击", "代充", "优惠券群", "低价充值",
  "佐匹克隆", "你是一个", "你是我的奴隶", "你是猫娘", "使用XX系统的都是", "俄乌战争",
  "修车", "傻X", "傻逼", "公知", "六合彩", "关注公众号", "冰毒", "利他林", "刷单",
  "刷流水", "加我微信", "南梁", "南海仲裁", "博彩", "双性恋", "反共", "反华", "发车",
  "口角", "台海", "右美沙芬", "叶子", "同性恋", "四爱", "垃圾系统", "复读接下来的话",
  "外围", "外围盘", "外挂", "大麻", "天安门", "女同", "孕酮", "孤儿", "实名", "小仙女",
  "小日本", "小金豆", "就是垃圾", "巴以冲突", "帮我助力", "广告", "开盒", "忽略之前的指令",
  "恋尸癖", "恋童癖", "恋足癖", "拼多多", "排泄", "文革", "日赚", "暴动", "曲马多",
  "未成年", "机场跑路", "极品", "枪支", "梯子", "棒子", "止咳水", "死全家", "河南人",
  "测速图", "海洛因", "涩图", "淘宝客", "渠道", "港脚", "游行", "漏点", "炒币", "煞笔",
  "燃料", "狗推", "狗都不用", "玩客云", "男娘", "百家乐", "盒", "看片", "睾酮", "砍一刀",
  "破解", "神仙水", "福利姬", "福利群", "网盘资源", "网赌", "美狗", "群号", "翻墙", "肛交",
  "脑瘫", "色图", "色普龙", "节点", "药", "药娘", "菠菜", "薅羊毛", "螺内酯", "补佳乐",
  "裸聊", "订阅链接", "走猫", "走线", "起义", "跨性别", "身份证", "车牌", "辅助",
  "过量服药", "进新群", "阿普唑仑", "隐私", "雌二醇", "飞行", "飞行员",
];
