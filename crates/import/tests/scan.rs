//! End-to-end scans over realistic fixtures written to a temporary home directory.

use std::path::{Path, PathBuf};
use std::process::Command;

use nanomgg_import::cli_hosts::{CliTool, TokenStorage};
use nanomgg_import::gitconfig::{IdentityScope, RuleKind, SigningFormat};
use nanomgg_import::{
    ImportReport, ProviderKind, ScanEnv, SuggestionNote, WarningKind, scan, scan_gh_hosts,
    scan_gitconfig, scan_glab_config, scan_ssh_config,
};

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn crlf(text: &str) -> String {
    text.replace('\n', "\r\n")
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

const GITCONFIG: &str = r#"[user]
	name = An Nguyen
	email = an@personal.dev   # personal
[include]
	path = .gitconfig-common
[includeIf "gitdir:~/work/"]
	path = .gitconfig-work
[includeIf "gitdir/i:D:\\Work\\Acme\\"]
	path = ~/.config/git/acme.inc
[includeIf "hasconfig:remote.*.url:git@gitlab.com:oss-group/**"]
	path = oss.gitconfig
[includeIf "onbranch:release/*"]
	path = .gitconfig-work
[includeIf "gitdir:~/missing/"]
	path = .gitconfig-nope
[alias]
	lg = "log --graph --format=\"%h %s\""
"#;

const COMMON: &str =
    "[gpg]\n\tformat = ssh\n[user]\n\tsigningkey = ~/.ssh/id_personal.pub\n[commit]\n\tgpgsign\n";

const WORK: &str = r#"[user]
    name = "An Nguyen (ACME)"
    email = an.nguyen@acme.vn
[core]
    sshCommand = ssh -i ~/.ssh/id_acme -o IdentitiesOnly=yes
[includeIf "gitdir:~/work/secret/"]
    path = work/secret.inc
"#;

const XDG: &str = "[user]\n\tname = XDG Name\n[includeIf \"gitdir:./repos/\"]\n\tpath = xdg.inc\n";

/// Builds the git config fixture; returns the home directory.
fn git_fixture(root: &Path) -> PathBuf {
    let home = root.join("home");
    write(&home.join(".gitconfig"), &crlf(GITCONFIG));
    write(&home.join(".gitconfig-common"), COMMON);
    write(&home.join(".gitconfig-work"), &crlf(WORK));
    write(
        &home.join("work").join("secret.inc"),
        "[user]\n\temail = an@secret.acme.vn\n",
    );
    write(
        &home.join(".config").join("git").join("acme.inc"),
        "[user]\n\temail = an@acme-corp.vn\n",
    );
    write(
        &home.join("oss.gitconfig"),
        "[user]\n\tname = an-oss\n\temail = an@users.noreply.gitlab.com\n",
    );
    write(&home.join(".config").join("git").join("config"), XDG);
    write(
        &home.join(".config").join("git").join("xdg.inc"),
        "[user]\n\temail = an@xdg.dev\n",
    );
    home
}

fn check_git_scan(home: &Path, git: Option<PathBuf>) {
    let env = ScanEnv {
        git,
        ..ScanEnv::default()
    };
    let scan = scan_gitconfig(home, &env);
    let h = slash(home);

    // Global identity: ~/.gitconfig wins over the XDG file; signing from the plain include.
    let global = &scan.identities[0];
    assert_eq!(global.scope, IdentityScope::Global);
    assert_eq!(global.name.as_deref(), Some("An Nguyen"));
    assert_eq!(global.email.as_deref(), Some("an@personal.dev"));
    assert_eq!(global.source.line, Some(3));
    assert!(global.source.file.ends_with(".gitconfig"));
    let signing = global.signing.as_ref().unwrap();
    assert_eq!(signing.format, SigningFormat::Ssh);
    assert_eq!(signing.key.as_deref(), Some("~/.ssh/id_personal.pub"));
    assert_eq!(
        signing.key_path.as_deref(),
        Some(format!("{h}/.ssh/id_personal.pub").as_str())
    );
    assert!(signing.sign_commits);
    assert!(!signing.sign_tags);

    let conds: Vec<&str> = scan.rules.iter().map(|r| r.condition.as_str()).collect();
    assert_eq!(
        conds,
        [
            "gitdir:./repos/",
            "gitdir:~/work/",
            "gitdir/i:D:\\Work\\Acme\\",
            "hasconfig:remote.*.url:git@gitlab.com:oss-group/**",
            "gitdir:~/work/secret/",
        ]
    );
    let ident = |i: usize| &scan.identities[scan.rules[i].identity_index.unwrap() as usize];

    let xdg = &scan.rules[0];
    assert_eq!(
        xdg.path_glob.as_deref(),
        Some(format!("{h}/.config/git/repos/**").as_str())
    );
    assert_eq!(ident(0).email.as_deref(), Some("an@xdg.dev"));

    let work = &scan.rules[1];
    assert_eq!(work.kind, RuleKind::PathGlob);
    assert_eq!(
        work.path_glob.as_deref(),
        Some(format!("{h}/work/**").as_str())
    );
    assert!(!work.case_insensitive);
    assert_eq!(work.source.line, Some(7));
    assert!(work.include_file.ends_with(".gitconfig-work"));
    let wi = ident(1);
    assert_eq!(wi.scope, IdentityScope::Conditional);
    assert_eq!(wi.condition.as_deref(), Some("gitdir:~/work/"));
    assert_eq!(wi.name.as_deref(), Some("An Nguyen (ACME)"));
    assert_eq!(wi.email.as_deref(), Some("an.nguyen@acme.vn"));
    assert_eq!(wi.source.line, Some(3));
    assert_eq!(
        wi.ssh_key_path.as_deref(),
        Some(format!("{h}/.ssh/id_acme").as_str())
    );
    assert!(wi.signing.is_none());

    let acme = &scan.rules[2];
    assert!(acme.case_insensitive);
    assert_eq!(acme.path_glob.as_deref(), Some("D:/Work/Acme/**"));
    assert_eq!(ident(2).email.as_deref(), Some("an@acme-corp.vn"));

    let oss = &scan.rules[3];
    assert_eq!(oss.kind, RuleKind::RemoteUrl);
    assert_eq!(oss.host.as_deref(), Some("gitlab.com"));
    assert_eq!(oss.owner_or_group.as_deref(), Some("oss-group"));
    assert_eq!(
        oss.remote_regex.as_deref(),
        Some("^git@gitlab\\.com:oss\\-group/.*$")
    );
    assert_eq!(ident(3).name.as_deref(), Some("an-oss"));

    let secret = &scan.rules[4];
    assert_eq!(secret.notes, [SuggestionNote::NestedCondition]);
    assert_eq!(
        secret.path_glob.as_deref(),
        Some(format!("{h}/work/secret/**").as_str())
    );
    assert_eq!(ident(4).email.as_deref(), Some("an@secret.acme.vn"));
    assert_eq!(scan.identities.len(), 6);

    let kinds: Vec<WarningKind> = scan.warnings.iter().map(|w| w.kind).collect();
    assert_eq!(
        kinds,
        [WarningKind::MissingInclude, WarningKind::Unsupported],
        "{:#?}",
        scan.warnings
    );
    assert_eq!(scan.warnings[0].source.line, Some(15));
    assert_eq!(scan.warnings[1].source.line, Some(13));
    // XDG, its include, .gitconfig, common, work, acme, oss, secret.
    assert_eq!(scan.files.len(), 8, "{:#?}", scan.files);
}

#[test]
fn gitconfig_with_includes_builtin_parser() {
    let root = tempfile::tempdir().unwrap();
    let home = git_fixture(root.path());
    check_git_scan(&home, None);
}

#[test]
fn gitconfig_with_includes_matches_git() {
    if !git_available() {
        eprintln!("git not found, skipping");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let home = git_fixture(root.path());
    check_git_scan(&home, Some(PathBuf::from("git")));
}

/// Our parser and `git config --list` agree on a nasty file (values, order, escapes).
#[test]
fn parser_agrees_with_git_on_tricky_syntax() {
    if !git_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tricky");
    let text = "[core]\r\n\tsshCommand = \"ssh -i \\\"C:\\\\Users\\\\An Nguyen\\\\.ssh\\\\id\\\"\"\r\n\
                [Section \"Sub \\\"q\\\" \\\\ x\"] key = v1 ; c\r\n\
                [a.B]\n\tk = a\\\n  b # c\n\tbool\n\tempty =\n\tsp =   lead and trail   \n\
                \ttab = x\\ty\n[url \"git@github.com-work:\"]\n\tinsteadOf = https://github.com/acme/\n";
    std::fs::write(&file, text).unwrap();
    let ours = nanomgg_import::gitconfig::parse_config(text);
    assert!(ours.errors.is_empty(), "{:?}", ours.errors);
    let out = Command::new("git")
        .args(["config", "--file"])
        .arg(&file)
        .args(["--list", "-z"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let theirs: Vec<(String, Option<String>)> = String::from_utf8(out.stdout)
        .unwrap()
        .split('\0')
        .filter(|r| !r.is_empty())
        .map(|r| match r.split_once('\n') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (r.to_string(), None),
        })
        .collect();
    let mine: Vec<(String, Option<String>)> = ours
        .entries
        .iter()
        .map(|e| (e.name.clone(), e.value.clone()))
        .collect();
    assert_eq!(mine, theirs);
    assert_eq!(
        mine[0].1.as_deref(),
        Some("ssh -i \"C:\\Users\\An Nguyen\\.ssh\\id\"")
    );
}

#[test]
fn gitconfig_edge_cases() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    // Self-include (cycle), invalid UTF-8 include, syntax error that does not stop the scan.
    write(
        &home.join(".gitconfig"),
        "[include]\n\tpath = ~/.gitconfig\n\tpath = bin.inc\n[user\n[user]\n\temail = x@y.z\n",
    );
    std::fs::write(home.join("bin.inc"), [0xff, 0xfe, 0x00, 0x41]).unwrap();
    let env = ScanEnv {
        git_config_global: None,
        ..ScanEnv::default()
    };
    let scan = scan_gitconfig(&home, &env);
    assert_eq!(scan.identities.len(), 1);
    assert_eq!(scan.identities[0].email.as_deref(), Some("x@y.z"));
    assert_eq!(scan.identities[0].name, None);
    let kinds: Vec<WarningKind> = scan.warnings.iter().map(|w| w.kind).collect();
    assert_eq!(
        kinds,
        [
            WarningKind::Parse,
            WarningKind::IncludeLoop,
            WarningKind::Unreadable
        ],
        "{:#?}",
        scan.warnings
    );

    // GIT_CONFIG_GLOBAL replaces both default files.
    let custom = root.path().join("custom.cfg");
    write(&custom, "[user]\n\tname = Custom\n");
    let env = ScanEnv {
        git_config_global: Some(custom),
        ..ScanEnv::default()
    };
    let scan = scan_gitconfig(&home, &env);
    assert_eq!(scan.identities[0].name.as_deref(), Some("Custom"));
    assert_eq!(scan.identities[0].email, None);

    // Nothing configured → nothing suggested, no warnings.
    let empty = root.path().join("empty-home");
    std::fs::create_dir_all(&empty).unwrap();
    let scan = scan_gitconfig(&empty, &ScanEnv::default());
    assert!(scan.identities.is_empty() && scan.warnings.is_empty() && scan.files.is_empty());
}

const SSH_CONFIG: &str = r#"# personal default
IdentityFile ~/.ssh/id_personal

Host github.com-work
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_acme
    IdentitiesOnly yes

Host gitlab-acme gl
    HostName=gitlab.acme.vn
    Port 2222
    IdentityFile "C:\Users\An Nguyen\.ssh\id gitlab"

Include conf.d/*.conf

Host *.corp !bad.corp
    IdentityFile %d/.ssh/id_corp

Match host foo
    User bar

Host *
    IdentitiesOnly no
    User fallback
"#;

fn ssh_fixture(home: &Path) {
    write(&home.join(".ssh").join("config"), &crlf(SSH_CONFIG));
    write(
        &home.join(".ssh").join("conf.d").join("bb.conf"),
        "Host bitbucket\n   HostName bitbucket.org\n   IdentityFile ~/.ssh/id_bb\n",
    );
    write(
        &home.join(".ssh").join("conf.d").join("readme.txt"),
        "Host ignored\n",
    );
    let key = nanomgg_ssh_keys::generate_ed25519("an@acme.vn", None).unwrap();
    write(&home.join(".ssh").join("id_acme"), &key.private_key);
    write(
        &home.join(".ssh").join("id_acme.pub"),
        &format!("{}\n", key.info.public_key),
    );
    write(&home.join(".ssh").join("id_personal"), "not read");
}

#[test]
fn ssh_config_hosts() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    ssh_fixture(&home);
    let h = slash(&home);
    let acme_pub = std::fs::read_to_string(home.join(".ssh").join("id_acme.pub")).unwrap();
    let acme_fp = nanomgg_ssh_keys::fingerprint(&acme_pub).unwrap();

    let scan = scan_ssh_config(&home, None);
    let aliases: Vec<&str> = scan.hosts.iter().map(|h| h.alias.as_str()).collect();
    assert_eq!(
        aliases,
        [
            "*",
            "github.com-work",
            "gitlab-acme",
            "gl",
            "bitbucket",
            "*.corp"
        ]
    );

    let default = &scan.hosts[0];
    assert!(default.wildcard);
    assert_eq!(default.notes, [SuggestionNote::WildcardHost]);
    assert_eq!(default.source.line, Some(2));
    assert_eq!(
        default.identity_files[0].path,
        format!("{h}/.ssh/id_personal")
    );
    assert!(default.identity_files[0].exists);
    assert_eq!(default.identity_files[0].fingerprint, None);

    let work = &scan.hosts[1];
    assert_eq!(work.host_name, "github.com");
    assert_eq!(work.provider, ProviderKind::Github);
    assert!(work.is_alias);
    assert_eq!(work.notes, [SuggestionNote::AliasUnnecessary]);
    assert_eq!(work.user.as_deref(), Some("git"));
    assert_eq!(work.identities_only, Some(true));
    assert_eq!(work.source.line, Some(4));
    let files: Vec<&str> = work
        .identity_files
        .iter()
        .map(|k| k.path.as_str())
        .collect();
    assert_eq!(
        files,
        [format!("{h}/.ssh/id_personal"), format!("{h}/.ssh/id_acme")]
    );
    let acme = &work.identity_files[1];
    assert_eq!(acme.fingerprint.as_deref(), Some(acme_fp.as_str()));
    assert_eq!(acme.algorithm.as_deref(), Some("ssh-ed25519"));
    assert_eq!(acme.comment.as_deref(), Some("an@acme.vn"));
    assert_eq!(acme.source.line, Some(7));

    let gl = &scan.hosts[2];
    assert_eq!(gl.host_name, "gitlab.acme.vn");
    assert_eq!(gl.provider, ProviderKind::GitlabSelfHosted);
    assert_eq!(gl.port, Some(2222));
    assert_eq!(gl.user.as_deref(), Some("fallback"));
    assert_eq!(gl.identities_only, Some(false));
    assert_eq!(
        gl.identity_files[1].path,
        "C:/Users/An Nguyen/.ssh/id gitlab"
    );
    assert!(gl.notes.contains(&SuggestionNote::KeyFileMissing));
    assert_eq!(scan.hosts[3].host_name, "gitlab.acme.vn");

    let bb = &scan.hosts[4];
    assert_eq!(bb.host_name, "bitbucket.org");
    assert_eq!(bb.provider, ProviderKind::Unknown);
    assert!(bb.source.file.ends_with("bb.conf"));
    assert_eq!(bb.source.line, Some(1));
    assert!(!bb.notes.contains(&SuggestionNote::AliasUnnecessary));

    let corp = &scan.hosts[5];
    assert!(corp.wildcard);
    assert_eq!(corp.identity_files[0].path, format!("{h}/.ssh/id_corp"));

    assert_eq!(scan.warnings.len(), 1);
    assert_eq!(scan.warnings[0].kind, WarningKind::Unsupported);
    assert_eq!(scan.warnings[0].source.line, Some(20));
    assert_eq!(scan.files.len(), 2);
}

#[test]
fn ssh_config_include_inside_host_block_and_missing_files() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    write(
        &home.join(".ssh").join("config"),
        "Host gh-alt\n  Include gh-alt.inc\n  User git\nInclude missing.conf\n",
    );
    write(
        &home.join(".ssh").join("gh-alt.inc"),
        "HostName ssh.github.com\nPort 443\n",
    );
    let scan = scan_ssh_config(&home, None);
    assert_eq!(scan.hosts.len(), 1);
    let h = &scan.hosts[0];
    assert_eq!(h.host_name, "ssh.github.com");
    assert_eq!(h.port, Some(443));
    assert_eq!(h.user.as_deref(), Some("git"));
    assert_eq!(h.provider, ProviderKind::Github);
    assert_eq!(scan.warnings[0].kind, WarningKind::MissingInclude);

    // No ssh config at all.
    let none = scan_ssh_config(&root.path().join("nobody"), None);
    assert!(none.hosts.is_empty() && none.warnings.is_empty());
}

const GH_HOSTS: &str = "github.com:
    users:
        an-personal:
        an-acme:
            oauth_token: gho_PLAINTEXTSECRET1
    git_protocol: ssh
    user: an-personal
ghe.acme.vn:
    oauth_token: gho_OLDSECRET2
    user: an
    git_protocol: https
";

const GLAB_CONFIG: &str = "# glab config
git_protocol: ssh
editor:
hosts:
    gitlab.com:
        token: glpat-SECRET3
        api_host: gitlab.com
        git_protocol: https
        api_protocol: https
        user: an
    \"gitlab.acme.vn\":
        token:
        api_host: gitlab-api.acme.vn
        user: an.nguyen
    gitlab.example.org:
        api_protocol: https
        token:
";

#[test]
fn gh_hosts_multi_account_and_legacy() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("hosts.yml");
    write(&path, &crlf(GH_HOSTS));
    let scan = scan_gh_hosts(&path);
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);
    type Row<'a> = (&'a str, Option<&'a str>, bool, TokenStorage, Option<u32>);
    let summary: Vec<Row> = scan
        .accounts
        .iter()
        .map(|a| {
            (
                a.host.as_str(),
                a.username.as_deref(),
                a.active,
                a.token_storage,
                a.source.line,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "github.com",
                Some("an-personal"),
                true,
                TokenStorage::Keyring,
                Some(3)
            ),
            (
                "github.com",
                Some("an-acme"),
                false,
                TokenStorage::ConfigFile,
                Some(4)
            ),
            (
                "ghe.acme.vn",
                Some("an"),
                true,
                TokenStorage::ConfigFile,
                Some(10)
            ),
        ]
    );
    assert!(scan.accounts.iter().all(|a| a.tool == CliTool::Gh));
    assert_eq!(scan.accounts[0].provider, ProviderKind::Github);
    assert_eq!(scan.accounts[2].provider, ProviderKind::GithubEnterprise);
    assert_eq!(scan.accounts[0].git_protocol.as_deref(), Some("ssh"));
    assert_eq!(scan.accounts[0].notes, [SuggestionNote::SignInAgain]);
    assert_eq!(
        scan.accounts[1].notes,
        [
            SuggestionNote::SignInAgain,
            SuggestionNote::TokenInPlainText
        ]
    );
    let json = serde_json::to_string(&scan).unwrap();
    assert!(!json.contains("SECRET"), "{json}");

    // Broken YAML → warning with a line, no panic.
    write(&path, "github.com:\n  user: [unclosed\n");
    let broken = scan_gh_hosts(&path);
    assert!(broken.accounts.is_empty());
    assert_eq!(broken.warnings[0].kind, WarningKind::Parse);
    // Empty file → nothing.
    write(&path, "");
    let empty = scan_gh_hosts(&path);
    assert!(empty.accounts.is_empty() && empty.warnings.is_empty());
}

#[test]
fn glab_hosts() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.yml");
    write(&path, GLAB_CONFIG);
    let scan = scan_glab_config(&path);
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);
    assert_eq!(scan.accounts.len(), 2);
    let com = &scan.accounts[0];
    assert_eq!(com.tool, CliTool::Glab);
    assert_eq!(com.provider, ProviderKind::Gitlab);
    assert_eq!(com.username.as_deref(), Some("an"));
    assert_eq!(com.api_host, None);
    assert_eq!(com.git_protocol.as_deref(), Some("https"));
    assert_eq!(com.token_storage, TokenStorage::ConfigFile);
    assert_eq!(com.source.line, Some(5));
    let acme = &scan.accounts[1];
    assert_eq!(acme.host, "gitlab.acme.vn");
    assert_eq!(acme.provider, ProviderKind::GitlabSelfHosted);
    assert_eq!(acme.api_host.as_deref(), Some("gitlab-api.acme.vn"));
    assert_eq!(acme.git_protocol.as_deref(), Some("ssh"));
    assert_eq!(acme.token_storage, TokenStorage::Unknown);
    assert_eq!(acme.source.line, Some(11));
    assert!(!serde_json::to_string(&scan).unwrap().contains("SECRET"));
}

#[test]
fn scan_everything_with_windows_layout() {
    let root = tempfile::tempdir().unwrap();
    let home = git_fixture(root.path());
    ssh_fixture(&home);
    let appdata = root.path().join("AppData").join("Roaming");
    write(&appdata.join("GitHub CLI").join("hosts.yml"), GH_HOSTS);
    write(&appdata.join("glab-cli").join("config.yml"), GLAB_CONFIG);
    // A stale Unix-style gh file must lose against %APPDATA% (gh's order).
    write(
        &home.join(".config").join("gh").join("hosts.yml"),
        "github.com:\n    user: stale\n",
    );

    let report: ImportReport = scan(&home, Some(&appdata), &ScanEnv::default());
    assert_eq!(report.identities.len(), 6);
    assert_eq!(report.rules.len(), 5);
    assert_eq!(report.ssh_hosts.len(), 6);
    assert_eq!(report.accounts.len(), 5);
    assert!(
        report
            .accounts
            .iter()
            .all(|a| a.username.as_deref() != Some("stale"))
    );
    assert_eq!(report.warnings.len(), 3);
    assert_eq!(report.scanned_files.len(), 8 + 2 + 2);
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("SECRET"));
    assert!(json.contains("\"identityIndex\":"));
    assert!(json.contains("\"tokenStorage\":\"keyring\""));

    // Unix layout: no AppData, XDG_CONFIG_HOME + GH_CONFIG_DIR honoured.
    let xdg = root.path().join("xdg");
    write(&xdg.join("glab-cli").join("config.yml"), GLAB_CONFIG);
    let gh_dir = root.path().join("ghdir");
    write(
        &gh_dir.join("hosts.yml"),
        "github.com:\n    user: from-env\n",
    );
    let env = ScanEnv {
        xdg_config_home: Some(xdg),
        gh_config_dir: Some(gh_dir),
        ..ScanEnv::default()
    };
    let report = scan(&home, None, &env);
    let users: Vec<&str> = report
        .accounts
        .iter()
        .filter_map(|a| a.username.as_deref())
        .collect();
    assert_eq!(users, ["from-env", "an", "an.nguyen"]);
}

/// Read-only smoke test against the real user configuration:
/// `cargo test -p nanomgg-import -- --ignored real_home --nocapture`. Prints counts only.
#[test]
#[ignore = "reads the developer's real ~/.gitconfig, ~/.ssh/config, gh and glab files"]
fn real_home_smoke() {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .expect("home directory");
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let report = scan(&home, appdata.as_deref(), &ScanEnv::from_process());
    let json = serde_json::to_string(&report).unwrap();
    for marker in ["gho_", "ghp_", "github_pat_", "glpat-"] {
        assert!(!json.contains(marker), "token-looking value leaked");
    }
    println!(
        "identities={} rules={} ssh_hosts={} accounts={} warnings={} files={}",
        report.identities.len(),
        report.rules.len(),
        report.ssh_hosts.len(),
        report.accounts.len(),
        report.warnings.len(),
        report.scanned_files.len()
    );
    for w in &report.warnings {
        println!("warning: {:?} {}", w.kind, w.message);
    }
}
