//! 署名:用户固定是 author(09-15 用户拍板),顾清影 以 `Co-Authored-By` trailer
//! 挂在 commit、PR 与 issue 正文末尾。trailer 由代码拼,不靠模型自觉。

use super::identity::BotAccount;
use crate::config::GithubToolConfig;

const DEFAULT_NAME: &str = "顾清影";

/// 名字在 Markdown 正文里链到的去处(09-16 用户拍板)。
const PROFILE_URL: &str = "https://github.com/yxxbc/gqy-agent";

/// 名字怎么写。commit message 是纯文本,写成 Markdown 只会看到一串方括号;
/// issue / PR 正文是 Markdown,名字可以点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameStyle {
    Plain,
    Linked,
}

/// 没登录 bot、也没配邮箱时的兜底。`.invalid` 是保留顶级域(RFC 2606),保证
/// 不会被 GitHub 关联到任何真实账号——宁可不挂头像,也不能挂错人。
pub(crate) const FALLBACK_EMAIL: &str = "gqy-agent@noreply.invalid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoAuthor {
    pub name: String,
    pub email: String,
}

impl CoAuthor {
    pub(crate) fn resolve(config: &GithubToolConfig, account: Option<&BotAccount>) -> Self {
        let base = config.coauthor_name.trim();
        // 09-16 用户拍板:名字后面不再缀「【模型】 (窗口)」。署名是她的名字,
        // 不是运行时铭牌;换个模型就换个署名,历史里同一个人看着像好几个。
        let name = if base.is_empty() { DEFAULT_NAME } else { base }.to_string();
        // ident 里的尖括号与换行会把 trailer 劈成两半。
        let name = name.replace(['<', '>', '\n', '\r'], "");
        let email = match config.coauthor_email.trim() {
            "" => account
                .map(BotAccount::noreply_email)
                .unwrap_or_else(|| FALLBACK_EMAIL.to_string()),
            email => email.to_string(),
        };
        Self { name, email }
    }

    pub(crate) fn trailer(&self) -> String {
        self.trailer_with(NameStyle::Plain)
    }

    pub(crate) fn trailer_with(&self, style: NameStyle) -> String {
        let name = match style {
            NameStyle::Plain => self.name.clone(),
            NameStyle::Linked => format!("[{}]({})", self.name, PROFILE_URL),
        };
        format!("Co-Authored-By: {} <{}>", name, self.email)
    }
}

/// 把 trailer 接到正文末尾。同一邮箱已经挂过就原样返回(模型手写过、或者
/// 重试同一条消息)。末段本身就是 trailer 块时紧贴着接,否则空一行另起。
///
/// `style` 只影响名字怎么写,去重认的始终是邮箱——所以纯文本版与链接版互相
/// 认得出,不会在同一段正文里叠两条。
pub(crate) fn append_trailer(text: &str, co_author: &CoAuthor, style: NameStyle) -> String {
    let body = text.trim_end();
    let needle = format!("<{}>", co_author.email.to_ascii_lowercase());
    let already = body.lines().any(|line| {
        let line = line.trim().to_ascii_lowercase();
        line.starts_with("co-authored-by:") && line.contains(&needle)
    });
    if already {
        return body.to_string();
    }
    let trailer = co_author.trailer_with(style);
    if body.is_empty() {
        return trailer;
    }
    // 只有一段时那一段是标题:`fix: foo` 长得和 trailer 一模一样,不能当 trailer 块。
    let joins_block = body
        .rsplit_once("\n\n")
        .is_some_and(|(_, last)| last.lines().all(is_trailer_line));
    let separator = if joins_block { "\n" } else { "\n\n" };
    format!("{body}{separator}{trailer}")
}

fn is_trailer_line(line: &str) -> bool {
    let Some((key, value)) = line.split_once(": ") else {
        return false;
    };
    key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !value.trim().is_empty()
}
