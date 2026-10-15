use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("got-cli-test-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git(args: &[&str]) -> Output {
    let output = Command::new("git")
        .args(args)
        .output()
        .expect("Git is required");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git_at(directory: &Path, args: &[&str]) -> Output {
    let mut command = Command::new("git");
    command.arg("-C").arg(directory).args(args);
    let output = command.output().expect("Git is required");
    assert!(
        output.status.success(),
        "git -C {} {:?} failed: {}",
        directory.display(),
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn text(output: Output) -> String {
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn command_help_describes_checkout_and_push_behavior() {
    let checkout_help = Command::new(env!("CARGO_BIN_EXE_got"))
        .args(["checkout", "--help"])
        .output()
        .unwrap();
    assert!(checkout_help.status.success());
    assert!(String::from_utf8_lossy(&checkout_help.stdout).contains("--revision"));
    assert!(String::from_utf8_lossy(&checkout_help.stdout).contains("blob filtering"));
    assert!(String::from_utf8_lossy(&checkout_help.stdout).contains("<remote> [<path>]"));
    assert!(String::from_utf8_lossy(&checkout_help.stdout).contains("whole repository"));

    let push_help = Command::new(env!("CARGO_BIN_EXE_got"))
        .args(["help", "push"])
        .output()
        .unwrap();
    assert!(push_help.status.success());
    assert!(String::from_utf8_lossy(&push_help.stdout).contains("does not rebase"));
    assert!(String::from_utf8_lossy(&push_help.stdout).contains("create a pull request"));
}

fn file_remote(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

#[test]
fn sparse_checkout_commit_and_push_preserve_unselected_paths() {
    let temp = TempDir::new();
    let source = temp.0.join("source");
    let remote = temp.0.join("remote.git");
    let checkout = temp.0.join("mono-apps-api");
    fs::create_dir_all(&source).unwrap();

    git(&["init", "--bare", remote.to_str().unwrap()]);
    git(&[
        "--git-dir",
        remote.to_str().unwrap(),
        "config",
        "uploadpack.allowFilter",
        "true",
    ]);
    git_at(&source, &["init"]);
    git_at(&source, &["config", "user.name", "Got Test"]);
    git_at(
        &source,
        &["config", "user.email", "got-test@example.invalid"],
    );
    git_at(&source, &["checkout", "-b", "main"]);
    fs::create_dir_all(source.join("apps/api")).unwrap();
    fs::create_dir_all(source.join("other")).unwrap();
    fs::write(source.join("README.md"), "root file\n").unwrap();
    fs::write(source.join("apps/api/handler.txt"), "before\n").unwrap();
    fs::write(source.join("other/data.txt"), "untouched\n").unwrap();
    git_at(&source, &["add", "."]);
    git_at(&source, &["commit", "-m", "initial"]);
    let base_sha = text(git_at(&source, &["rev-parse", "HEAD"]));
    fs::write(source.join("README.md"), "latest branch tip\n").unwrap();
    git_at(&source, &["add", "."]);
    git_at(&source, &["commit", "-m", "advance base before checkout"]);
    git_at(
        &source,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git_at(&source, &["push", "-u", "origin", "main"]);
    git(&[
        "--git-dir",
        remote.to_str().unwrap(),
        "symbolic-ref",
        "HEAD",
        "refs/heads/main",
    ]);
    let remote_url = file_remote(&remote);
    let incomplete_checkout = temp.0.join("incomplete-checkout");
    let failed_checkout = Command::new(env!("CARGO_BIN_EXE_got"))
        .args([
            "checkout",
            &remote_url,
            "missing/path",
            "--directory",
            incomplete_checkout.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!failed_checkout.status.success());
    assert!(!incomplete_checkout.exists());

    let latest_checkout = temp.0.join("mono-apps-api-latest");
    let latest_checkout_output = Command::new(env!("CARGO_BIN_EXE_got"))
        .args([
            "checkout",
            &remote_url,
            "apps/api",
            "--directory",
            latest_checkout.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        latest_checkout_output.status.success(),
        "got checkout failed: {}",
        String::from_utf8_lossy(&latest_checkout_output.stderr)
    );
    assert_eq!(
        text(git_at(
            &latest_checkout,
            &["rev-parse", "--is-shallow-repository"]
        )),
        "true"
    );
    assert_eq!(
        text(git_at(
            &latest_checkout,
            &["config", "--bool", "--get", "remote.origin.promisor"]
        )),
        "true"
    );
    assert_eq!(
        text(git_at(
            &latest_checkout,
            &["config", "--get", "remote.origin.partialclonefilter"]
        )),
        "blob:none"
    );
    assert!(latest_checkout.join("apps/api/handler.txt").is_file());
    assert!(!latest_checkout.join("other/data.txt").exists());

    let full_checkout = temp.0.join("mono");
    let full_checkout_output = Command::new(env!("CARGO_BIN_EXE_got"))
        .args([
            "checkout",
            &remote_url,
            "--directory",
            full_checkout.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        full_checkout_output.status.success(),
        "got checkout failed: {}",
        String::from_utf8_lossy(&full_checkout_output.stderr)
    );
    assert!(String::from_utf8_lossy(&full_checkout_output.stdout)
        .contains("Checkout scope: repository root (all tracked files)"));
    assert!(full_checkout.join("README.md").is_file());
    assert!(full_checkout.join("apps/api/handler.txt").is_file());
    assert!(full_checkout.join("other/data.txt").is_file());
    assert_eq!(
        text(git_at(
            &full_checkout,
            &["rev-parse", "--is-shallow-repository"]
        )),
        "true"
    );
    let partial_clone_config = Command::new("git")
        .args([
            "-C",
            full_checkout.to_str().unwrap(),
            "config",
            "--get",
            "remote.origin.promisor",
        ])
        .output()
        .unwrap();
    assert!(
        !partial_clone_config.status.success(),
        "root checkout should fetch blobs up front rather than use a partial clone"
    );

    let checkout_output = Command::new(env!("CARGO_BIN_EXE_got"))
        .args([
            "checkout",
            &remote_url,
            "apps/api",
            "--revision",
            &base_sha,
            "--directory",
            checkout.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        checkout_output.status.success(),
        "got checkout failed: {}",
        String::from_utf8_lossy(&checkout_output.stderr)
    );
    assert!(String::from_utf8_lossy(&checkout_output.stdout)
        .contains(&format!("Next: cd \"{}\"", checkout.join("apps/api").display())));
    assert!(checkout.join("apps/api/handler.txt").is_file());
    assert!(!checkout.join("other/data.txt").exists());
    assert_eq!(text(git_at(&checkout, &["rev-parse", "HEAD"])), base_sha);
    assert_eq!(
        text(git_at(
            &checkout,
            &["config", "--bool", "--get", "remote.origin.promisor"]
        )),
        "true"
    );

    git_at(&checkout, &["config", "user.name", "Got Test"]);
    git_at(
        &checkout,
        &["config", "user.email", "got-test@example.invalid"],
    );
    fs::write(checkout.join("apps/api/handler.txt"), "after\n").unwrap();
    let add = Command::new(env!("CARGO_BIN_EXE_got"))
        .current_dir(&checkout)
        .args(["add", "apps/api/handler.txt"])
        .status()
        .unwrap();
    assert!(add.success());
    let commit = Command::new(env!("CARGO_BIN_EXE_got"))
        .current_dir(&checkout)
        .args(["commit", "-m", "update api"])
        .status()
        .unwrap();
    assert!(commit.success());
    let parent = text(git_at(&checkout, &["show", "-s", "--format=%P", "HEAD"]));
    assert_eq!(parent, base_sha);

    fs::write(source.join("other/new.txt"), "base advanced\n").unwrap();
    git_at(&source, &["add", "."]);
    git_at(&source, &["commit", "-m", "advance base"]);
    git_at(&source, &["push", "origin", "main"]);

    let push = Command::new(env!("CARGO_BIN_EXE_got"))
        .current_dir(&checkout)
        .arg("push")
        .output()
        .unwrap();
    assert!(
        push.status.success(),
        "got push failed: {}",
        String::from_utf8_lossy(&push.stderr)
    );
    assert!(String::from_utf8_lossy(&push.stderr).contains("base branch 'main' advanced"));

    let work_branch = text(git_at(&checkout, &["branch", "--show-current"]));
    assert_eq!(
        text(git(&[
            "--git-dir",
            remote.to_str().unwrap(),
            "show",
            &format!("{work_branch}:apps/api/handler.txt"),
        ])),
        "after"
    );
    assert_eq!(
        text(git(&[
            "--git-dir",
            remote.to_str().unwrap(),
            "show",
            &format!("{work_branch}:other/data.txt"),
        ])),
        "untouched"
    );
}
