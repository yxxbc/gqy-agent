//! 署名:用户固定是 author(09-15 用户拍板),顾清影 以 `Co-Authored-By` trailer
//! 挂在 commit、PR 与 issue 正文末尾。trailer 由代码拼,不靠模型自觉。

use super::identity::BotAccount;
use crate::config::GithubToolConfig;
use crate::tools::workspace::TurnModel;

const DEFAULT_NAME: &str = "顾清影";

/// 没登录 bot、也没配邮箱时的兜底。`.invalid` 是保留顶级域(RFC 2606),保证
/// 不会被 GitHub 关联到任何真实账号——宁可不挂头像,也不能挂错人。
pub(crate) const FALLBACK_EMAIL: &str = "gqy-agent@noreply.invalid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoAuthor {
    pub name: String,
    pub email: String,
}

impl CoAuthor {
    pub(crate) fn resolve(
        config: &GithubToolConfig,
        account: Option<&BotAccount>,
        model: Option<&TurnModel>,
    ) -> Self {
        let base = config.coauthor_name.trim();
        let mut name = if base.is_empty() { DEFAULT_NAME } else { base }.to_string();
        if let Some(model) = model.filter(|model| !model.model.trim().is_empty()) {
            name.push_str(&format!("【{}】", model.model.trim()));
            if let Some(window) = model.context_window {
                name.push_str(&format!(" ({})", format_window(window)));
            }
        }
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
        format!("Co-Authored-By: {} <{}>", self.name, self.email)
    }
}

pub(crate) fn format_window(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        let millions = format!("{:.1}", tokens as f64 / 1_000_000.0);
        format!("{}M", millions.trim_end_matches(".0"))
    } else if tokens >= 1_000 {
        format!("{}K", (tokens + 500) / 1_000)
    } else {
        tokens.to_string()
    }
}

/// 把 trailer 接到正文末尾。同一邮箱已经挂过就原样返回(模型手写过、或者
/// 重试同一条消息)。末段本身就是 trailer 块时紧贴着接,否则空一行另起。
pub(crate) fn append_trailer(text: &str, co_author: &CoAuthor) -> String {
    let body = text.trim_end();
    let needle = format!("<{}>", co_author.email.to_ascii_lowercase());
    let already = body.lines().any(|line| {
        let line = line.trim().to_ascii_lowercase();
        line.starts_with("co-authored-by:") && line.contains(&needle)
    });
    if already {
        return body.to_string();
    }
    let trailer = co_author.trailer();
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
