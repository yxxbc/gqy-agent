use super::attribution::{append_trailer, CoAuthor, NameStyle, FALLBACK_EMAIL};
use super::identity::{gitconfig_text, hosts_token, BotAccount, BotHome};
use crate::config::GithubToolConfig;
use std::ffi::OsStr;

fn account() -> BotAccount {
    BotAccount {
        login: "gqy-bot".to_string(),
        id: 42,
    }
}

fn co_author(email: &str) -> CoAuthor {
    CoAuthor {
        name: "顾清影".to_string(),
        email: email.to_string(),
    }
}

#[test]
fn co_author_is_just_the_name() {
    let resolved = CoAuthor::resolve(&GithubToolConfig::default(), Some(&account()));
    assert_eq!(
        resolved.trailer(),
        "Co-Authored-By: 顾清影 <42+gqy-bot@users.noreply.github.com>"
    );
}

#[test]
fn linked_style_wraps_only_the_name() {
    let resolved = CoAuthor::resolve(&GithubToolConfig::default(), Some(&account()));
    assert_eq!(
        resolved.trailer_with(NameStyle::Linked),
        "Co-Authored-By: [顾清影](https://github.com/yxxbc/gqy-agent) \
<42+gqy-bot@users.noreply.github.com>"
    );
}

#[test]
fn co_author_email_prefers_config_then_bot_then_fallback() {
    let mut config = GithubToolConfig::default();
    assert_eq!(CoAuthor::resolve(&config, None).email, FALLBACK_EMAIL);
    assert_eq!(
        CoAuthor::resolve(&config, Some(&account())).email,
        "42+gqy-bot@users.noreply.github.com"
    );
    config.coauthor_email = "me@example.com".to_string();
    assert_eq!(
        CoAuthor::resolve(&config, Some(&account())).email,
        "me@example.com"
    );
}

#[test]
fn co_author_name_cannot_break_the_ident() {
    let mut config = GithubToolConfig::default();
    config.coauthor_name = "evil<name>\nx".to_string();
    let resolved = CoAuthor::resolve(&config, None);
    assert!(!resolved.name.contains(['<', '>', '\n']));
}

#[test]
fn trailer_goes_after_a_blank_line() {
    let co = co_author("a@b.c");
    assert_eq!(
        append_trailer("fix: handle empty config", &co, NameStyle::Plain),
        "fix: handle empty config\n\nCo-Authored-By: 顾清影 <a@b.c>"
    );
    assert_eq!(
        append_trailer("fix: x\n\nLonger body.\n", &co, NameStyle::Plain),
        "fix: x\n\nLonger body.\n\nCo-Authored-By: 顾清影 <a@b.c>"
    );
}

#[test]
fn trailer_joins_an_existing_trailer_block() {
    let co = co_author("a@b.c");
    assert_eq!(
        append_trailer("fix: x\n\nSigned-off-by: Me <me@x.y>", &co, NameStyle::Plain),
        "fix: x\n\nSigned-off-by: Me <me@x.y>\nCo-Authored-By: 顾清影 <a@b.c>"
    );
}

#[test]
fn trailer_is_idempotent_per_email() {
    let co = co_author("a@b.c");
    let once = append_trailer("fix: x", &co, NameStyle::Plain);
    assert_eq!(append_trailer(&once, &co, NameStyle::Plain), once);
    let manual = "fix: x\n\nco-authored-by: someone <A@B.C>";
    assert_eq!(append_trailer(manual, &co, NameStyle::Plain), manual);
}

#[test]
fn empty_body_becomes_the_trailer() {
    assert_eq!(
        append_trailer("  \n", &co_author("a@b.c"), NameStyle::Plain),
        "Co-Authored-By: 顾清影 <a@b.c>"
    );
}

#[test]
fn bot_gitconfig_resets_inherited_credential_helpers() {
    let text = gitconfig_text(&account());
    assert!(text.contains("email = 42+gqy-bot@users.noreply.github.com"));
    assert!(text.contains("[credential]\n\thelper =\n"));
    assert!(text.contains("\thelper =\n\thelper = !gh auth git-credential\n"));
}

#[test]
fn bot_env_isolates_from_host_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::tools::tests::test_paths(temp.path());
    let home = BotHome::new(&paths);
    home.save_account(&account()).unwrap();
    write_hosts(&home, HOSTS_LEGACY);
    let credentials = home.credentials().unwrap();
    let mut command = std::process::Command::new("gh");
    home.apply_bot(&mut command, &credentials);
    let envs: Vec<(&OsStr, Option<&OsStr>)> = command.get_envs().collect();
    let get = |key: &str| {
        envs.iter()
            .find(|(name, _)| *name == OsStr::new(key))
            .map(|(_, value)| *value)
    };
    // 宿主 token 剥掉,bot token 显式给上:gh 不会再回退到 Keychain。
    assert_eq!(get("GITHUB_TOKEN"), Some(None));
    assert_eq!(get("GH_TOKEN"), Some(Some(OsStr::new("gho_bot"))));
    assert_eq!(
        get("GH_CONFIG_DIR"),
        Some(Some(temp.path().join("github/gh").as_os_str()))
    );
    assert_eq!(get("GIT_CONFIG_NOSYSTEM"), Some(Some(OsStr::new("1"))));
    assert_eq!(get("GIT_SSH_COMMAND"), Some(Some(OsStr::new("false"))));
    assert_eq!(
        get("GIT_COMMITTER_EMAIL"),
        Some(Some(OsStr::new("42+gqy-bot@users.noreply.github.com")))
    );
}

#[test]
fn account_round_trips_and_clears() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::tools::tests::test_paths(temp.path());
    let home = BotHome::new(&paths);
    assert_eq!(home.account(), None);
    home.save_account(&account()).unwrap();
    assert_eq!(home.account(), Some(account()));
    assert!(home.gitconfig_path().exists());
    home.clear_account().unwrap();
    assert_eq!(home.account(), None);
    assert!(!home.gitconfig_path().exists());
    home.clear_account().unwrap();
}

const HOSTS_LEGACY: &str =
    "github.com:\n    git_protocol: https\n    oauth_token: gho_bot\n    user: gqy-bot\n";

const HOSTS_MULTI: &str = "github.com:\n    users:\n        gqy-bot:\n            oauth_token: gho_multi\n    git_protocol: https\n    user: gqy-bot\n";

fn write_hosts(home: &BotHome, raw: &str) {
    home.ensure_dirs().unwrap();
    std::fs::write(home.gh_config_dir().join("hosts.yml"), raw).unwrap();
}

#[test]
fn hosts_token_reads_both_gh_layouts() {
    assert_eq!(hosts_token(HOSTS_LEGACY).as_deref(), Some("gho_bot"));
    assert_eq!(hosts_token(HOSTS_MULTI).as_deref(), Some("gho_multi"));
    assert_eq!(hosts_token("github.com:\n    user: gqy-bot\n"), None);
    assert_eq!(hosts_token(""), None);
}

/// 回归(09-15 实测):bot 目录里没存 token 时 gh 会回退到 Keychain 里宿主的
/// token。有账号记录但没 token 必须拒绝,绝不能放行到 gh。
#[test]
fn bot_without_a_stored_token_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::tools::tests::test_paths(temp.path());
    let home = BotHome::new(&paths);
    home.save_account(&account()).unwrap();
    let error = home.credentials().err().expect("must refuse");
    assert!(format!("{error:#}").contains("no stored token"));
    write_hosts(&home, HOSTS_MULTI);
    assert!(home.credentials().is_ok());
}
