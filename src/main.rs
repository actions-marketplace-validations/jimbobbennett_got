use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "\
got - fast, folder-scoped Git checkouts

Usage:
  got checkout <remote> [<path>] [OPTIONS]
  got status [GIT-OPTIONS]
  got add <pathspec>...
  got commit [GIT-OPTIONS]
  got push [GIT-ARGUMENTS...]
  got help [COMMAND]
  got --version

Commands:
  checkout  Create a shallow checkout, optionally sparse, and a work branch
  status    Run git status in the current repository
  add       Run git add in the current repository
  commit    Run git commit in the current repository
  push      Push with Git; a bare push safely selects the work-branch remote
  help      Show general or command-specific help

Help:
  got help checkout
  got checkout --help
  got help push

Run 'got help <command>' for details. Git must be installed and available on PATH.";

const CHECKOUT_HELP: &str = "\
got checkout - create a shallow checkout of a repository or one directory

Usage:
  got checkout <remote> [<path>] [OPTIONS]

Arguments:
  <remote>  Git remote URL or GitHub shorthand such as owner/repository
  <path>    Optional repository-relative directory, using '/' separators.
            Omit it to check out the whole repository.

Options:
  --branch <branch>           Base branch (default: remote's default branch)
  --revision <commit>         Commit reachable from the selected base branch
  --directory <destination>   Checkout destination (default: <repo> or <repo>-<path>)
  -h, --help                  Show this help

Behavior:
  * Uses the latest tip of the selected base branch unless --revision is set.
  * Creates a shallow checkout, then a new local work branch based on that
    commit. Omitting <path> checks out all tracked files; supplying <path>
    creates a sparse checkout. Existing destination paths are never overwritten.
  * Uses blob filtering when supported. If the remote rejects or ignores the
    filter, sparse checkouts remain shallow and print a transfer warning.
  * Cone-mode sparse checkout also includes root files and parent-directory
    files required by Git. Commits remain full-repository Git commits.
  * A supplied revision must be reachable from the selected base branch;
    history is deepened only as needed to check reachability.

Examples:
  got checkout https://github.com/acme/mono.git
  got checkout https://github.com/acme/mono.git apps/api
  got checkout acme/mono apps/api --branch main --revision abc123
  got checkout acme/mono apps/api --directory ../mono-api

After checkout, change into the printed checkout or selected-path directory,
then use 'got status', 'got add', 'got commit', and 'got push'.";

const STATUS_HELP: &str = "\
got status - show the current repository status

Usage:
  got status [GIT-OPTIONS]

Arguments are passed unchanged to 'git status'. For Git's full option list,
run 'got status --help'.";

const ADD_HELP: &str = "\
got add - stage paths for the next commit

Usage:
  got add <pathspec>...

Arguments are passed unchanged to 'git add'. For Git's full option list,
run 'got add --help'.";

const COMMIT_HELP: &str = "\
got commit - create a commit from the current index

Usage:
  got commit [GIT-OPTIONS]

Arguments are passed unchanged to 'git commit'; unstaged changes are not
automatically included unless a Git option such as -a requests that. For
Git's full option list, run 'got commit --help'.";

const PUSH_HELP: &str = "\
got push - publish the current branch

Usage:
  got push [GIT-ARGUMENTS...]

With arguments, they are passed unchanged to 'git push'.
With no arguments, got selects the configured upstream or checkout remote,
sets the upstream on the first push, and warns if the recorded base branch
has moved. It pushes only the work branch; it does not rebase, force-push,
update the base branch, or create a pull request.

For Git's full option list, run 'got push --help'.";

#[derive(Debug, PartialEq, Eq)]
struct CheckoutOptions {
    remote: String,
    path: String,
    branch: Option<String>,
    revision: Option<String>,
    directory: Option<PathBuf>,
}

#[derive(Debug)]
struct BaseRef {
    branch: String,
    sha: String,
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("got: {message}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32, String> {
    let mut args = env::args_os().skip(1);
    let Some(command) = args.next() else {
        println!("{USAGE}");
        return Ok(0);
    };

    let command = command
        .to_str()
        .ok_or_else(|| "command name must be valid UTF-8".to_owned())?;
    let remaining: Vec<OsString> = args.collect();

    match command {
        "checkout" if has_help_flag(&remaining) => {
            println!("{CHECKOUT_HELP}");
            Ok(0)
        }
        "checkout" => {
            let options = parse_checkout_args(&remaining)?;
            checkout(options)?;
            Ok(0)
        }
        "status" => run_git_passthrough("status", &remaining),
        "add" => run_git_passthrough("add", &remaining),
        "commit" => run_git_passthrough("commit", &remaining),
        "push" if !remaining.is_empty() => run_git_passthrough("push", &remaining),
        "push" => push_current_branch(),
        "--help" | "-h" => {
            println!("{USAGE}");
            Ok(0)
        }
        "help" => {
            if remaining.is_empty() {
                println!("{USAGE}");
                return Ok(0);
            }
            if remaining.len() != 1 {
                return Err("usage: got help [COMMAND]".to_owned());
            }
            let topic = remaining[0]
                .to_str()
                .ok_or_else(|| "help topic must be valid UTF-8".to_owned())?;
            if matches!(topic, "-h" | "--help") {
                println!("{USAGE}");
                return Ok(0);
            }
            let help = command_help(topic)
                .ok_or_else(|| format!("unknown help topic '{topic}'\n\n{USAGE}"))?;
            println!("{help}");
            Ok(0)
        }
        "--version" | "-V" => {
            println!("got {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        _ => Err(format!("unknown command '{command}'\n\n{USAGE}")),
    }
}

fn has_help_flag(args: &[OsString]) -> bool {
    args.iter()
        .any(|argument| argument == "--help" || argument == "-h")
}

fn command_help(command: &str) -> Option<&'static str> {
    match command {
        "checkout" => Some(CHECKOUT_HELP),
        "status" => Some(STATUS_HELP),
        "add" => Some(ADD_HELP),
        "commit" => Some(COMMIT_HELP),
        "push" => Some(PUSH_HELP),
        "help" => Some(USAGE),
        _ => None,
    }
}

fn parse_checkout_args(args: &[OsString]) -> Result<CheckoutOptions, String> {
    if args.is_empty() {
        return Err(format!("checkout requires <remote>\n\n{USAGE}"));
    }

    let remote = args[0]
        .to_str()
        .ok_or_else(|| "remote must be valid UTF-8".to_owned())?
        .to_owned();

    if remote.is_empty() || remote.starts_with('-') {
        return Err("remote must be a Git URL or repository shorthand".to_owned());
    }

    let (path, mut index) = match args.get(1) {
        Some(value) => {
            let value = value
                .to_str()
                .ok_or_else(|| "path or option must be valid UTF-8".to_owned())?;
            if value.starts_with('-') {
                (".".to_owned(), 1)
            } else {
                validate_repo_path(value)?;
                (value.to_owned(), 2)
            }
        }
        None => (".".to_owned(), 1),
    };

    let mut branch = None;
    let mut revision = None;
    let mut directory = None;

    while index < args.len() {
        let option = args[index]
            .to_str()
            .ok_or_else(|| "option must be valid UTF-8".to_owned())?;
        index += 1;
        if index == args.len() {
            return Err(format!("missing value for {option}"));
        }
        let value = &args[index];
        let value = value
            .to_str()
            .ok_or_else(|| format!("{option} value must be valid UTF-8"))?
            .to_owned();
        index += 1;

        match option {
            "--branch" => set_once(&mut branch, value, option)?,
            "--revision" => set_once(&mut revision, value, option)?,
            "--directory" => set_once(&mut directory, PathBuf::from(value), option)?,
            _ => return Err(format!("unknown checkout option '{option}'\n\n{USAGE}")),
        }
    }

    if let Some(branch) = &branch {
        if branch.is_empty() {
            return Err("--branch cannot be empty".to_owned());
        }
    }
    if let Some(revision) = &revision {
        if revision.is_empty() {
            return Err("--revision cannot be empty".to_owned());
        }
    }
    if let Some(directory) = &directory {
        if directory.as_os_str().is_empty() {
            return Err("--directory cannot be empty".to_owned());
        }
    }

    Ok(CheckoutOptions {
        remote,
        path,
        branch,
        revision,
        directory,
    })
}

fn set_once<T>(slot: &mut Option<T>, value: T, option: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("{option} may only be specified once"));
    }
    *slot = Some(value);
    Ok(())
}

fn validate_repo_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || path.split('/').any(|component| {
            component.is_empty()
                || component == "."
                || component == ".."
                || component.eq_ignore_ascii_case(".git")
        })
    {
        return Err(
            "path must be a non-empty repository-relative directory using '/' separators"
                .to_owned(),
        );
    }
    Ok(())
}

fn checkout(options: CheckoutOptions) -> Result<(), String> {
    let remote = normalize_remote(&options.remote);
    reject_embedded_credentials(&remote)?;
    let base_ref = resolve_base_ref(&remote, options.branch.as_deref())?;
    let directory = options
        .directory
        .clone()
        .unwrap_or_else(|| PathBuf::from(default_directory_name(&remote, &options.path)));

    match fs::symlink_metadata(&directory) {
        Ok(_) => {
            return Err(format!(
                "destination '{}' already exists; choose another with --directory",
                directory.display()
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "could not inspect destination '{}': {error}",
                directory.display()
            ))
        }
    }
    let parent = directory
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    if let Some(parent) = parent {
        if !parent.is_dir() {
            return Err(format!(
                "destination parent '{}' does not exist",
                parent.display()
            ));
        }
    }

    let partial_clone =
        clone_repository(&remote, &base_ref.branch, &directory, options.path != ".")?;
    match finish_checkout(&options, &base_ref, &directory, partial_clone) {
        Ok(()) => Ok(()),
        Err(error) => match remove_partial_checkout(&directory) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(format!("{error}; {cleanup_error}")),
        },
    }
}

fn finish_checkout(
    options: &CheckoutOptions,
    base_ref: &BaseRef,
    directory: &Path,
    partial_clone: bool,
) -> Result<(), String> {
    let fetched_base = rev_parse(
        directory,
        &format!("refs/remotes/origin/{}", base_ref.branch),
    )?;
    if fetched_base != base_ref.sha {
        return Err(format!(
            "branch '{}' changed while the checkout was being created; run the command again",
            base_ref.branch
        ));
    }

    let selected_revision = match options.revision.as_deref() {
        Some(revision) => resolve_revision(directory, &base_ref.branch, &base_ref.sha, revision)?,
        None => base_ref.sha.clone(),
    };

    if options.path != "." {
        verify_directory(directory, &selected_revision, &options.path)?;
        git_checked(
            Some(directory),
            strings(&["sparse-checkout", "init", "--cone"]),
            "configure sparse checkout",
        )?;
        git_checked(
            Some(directory),
            vec![
                "sparse-checkout".into(),
                "set".into(),
                "--".into(),
                options.path.clone().into(),
            ],
            "set sparse checkout path",
        )?;
    }

    let work_branch = work_branch_name(&options.path, &selected_revision);
    git_checked(
        Some(directory),
        vec![
            "switch".into(),
            "--create".into(),
            work_branch.clone().into(),
            selected_revision.clone().into(),
        ],
        "create work branch",
    )?;
    set_got_config(directory, "got.base-branch", &base_ref.branch)?;
    set_got_config(directory, "got.base-sha", &selected_revision)?;
    set_got_config(directory, "got.base-tip-sha", &base_ref.sha)?;
    set_got_config(directory, "got.remote", "origin")?;
    set_got_config(directory, "got.path", &options.path)?;

    if !partial_clone && options.path != "." {
        eprintln!(
            "warning: the remote did not honor blob filtering; unselected blobs may have been transferred"
        );
    }
    println!("Base branch: {}", base_ref.branch);
    println!("Base commit: {selected_revision}");
    if options.path == "." {
        println!("Checkout scope: repository root (all tracked files)");
    } else {
        println!("Sparse path: {}", options.path);
    }
    println!("Work branch: {work_branch}");
    println!("Checkout: {}", directory.display());
    if options.path == "." {
        println!("Next: cd \"{}\"", directory.display());
    } else {
        println!("Next: cd \"{}\"", directory.join(&options.path).display());
    }
    Ok(())
}

fn normalize_remote(remote: &str) -> String {
    if remote.contains("://")
        || remote.starts_with("git@")
        || remote.starts_with("./")
        || remote.starts_with("../")
        || remote.starts_with('/')
    {
        return remote.to_owned();
    }

    let parts: Vec<&str> = remote.split('/').collect();
    if parts.len() == 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && *part != "." && *part != "..")
    {
        format!("https://github.com/{remote}.git")
    } else {
        remote.to_owned()
    }
}

fn reject_embedded_credentials(remote: &str) -> Result<(), String> {
    if let Some((scheme, rest)) = remote.split_once("://") {
        let authority = rest.split('/').next().unwrap_or_default();
        if matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
            && authority.contains('@')
        {
            return Err(format!(
                "do not embed credentials in the {scheme} URL; use Git's credential helper or SSH"
            ));
        }
    }
    Ok(())
}

fn resolve_base_ref(remote: &str, requested_branch: Option<&str>) -> Result<BaseRef, String> {
    match requested_branch {
        Some(branch) => {
            git_checked(
                None,
                vec!["check-ref-format".into(), "--branch".into(), branch.into()],
                "validate branch name",
            )?;
            let full_ref = format!("refs/heads/{branch}");
            let output = git_checked(
                None,
                vec![
                    "ls-remote".into(),
                    "--heads".into(),
                    remote.into(),
                    full_ref.clone().into(),
                ],
                "resolve remote branch",
            )?;
            let sha = find_ref_sha(&output.stdout, &full_ref).ok_or_else(|| {
                format!("remote branch '{branch}' does not exist or is not visible")
            })?;
            Ok(BaseRef {
                branch: branch.to_owned(),
                sha,
            })
        }
        None => {
            let output = git_checked(
                None,
                vec![
                    "ls-remote".into(),
                    "--symref".into(),
                    remote.into(),
                    "HEAD".into(),
                ],
                "resolve remote default branch",
            )?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let branch = stdout.lines().find_map(|line| {
                let (target, name) = line.split_once('\t')?;
                if name == "HEAD" {
                    target.strip_prefix("ref: refs/heads/").map(str::to_owned)
                } else {
                    None
                }
            });
            let sha = find_ref_sha(&output.stdout, "HEAD");
            match (branch, sha) {
                (Some(branch), Some(sha)) => Ok(BaseRef { branch, sha }),
                _ => Err(
                    "could not determine the remote's default branch; specify it with --branch"
                        .to_owned(),
                ),
            }
        }
    }
}

fn find_ref_sha(output: &[u8], expected_ref: &str) -> Option<String> {
    String::from_utf8_lossy(output).lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let sha = fields.next()?;
        let name = fields.next()?;
        (name == expected_ref).then(|| sha.to_owned())
    })
}

fn clone_repository(
    remote: &str,
    branch: &str,
    directory: &Path,
    sparse: bool,
) -> Result<bool, String> {
    let args = clone_args(remote, branch, directory, sparse, sparse);

    let (status, stderr) = run_git_stream(&args)?;
    if !status.success() {
        if !sparse || !can_retry_unfiltered(&stderr) {
            remove_partial_checkout(directory)?;
            return Err(format!(
                "could not clone repository: {}",
                String::from_utf8_lossy(&stderr).trim()
            ));
        }

        eprintln!("warning: remote rejected blob filtering; retrying with a shallow clone");
        remove_partial_checkout(directory)?;
        let fallback = clone_args(remote, branch, directory, sparse, false);
        let (fallback_status, fallback_stderr) = run_git_stream(&fallback)?;
        if !fallback_status.success() {
            remove_partial_checkout(directory)?;
            return Err(format!(
                "could not clone repository: {}",
                String::from_utf8_lossy(&fallback_stderr).trim()
            ));
        }
        return Ok(false);
    }

    let stderr_text = String::from_utf8_lossy(&stderr).to_lowercase();
    if stderr_text.contains("filter") && stderr_text.contains("ignoring") {
        return Ok(false);
    }
    is_partial_clone(directory)
}

fn clone_args(
    remote: &str,
    branch: &str,
    directory: &Path,
    sparse: bool,
    filter_blobs: bool,
) -> Vec<OsString> {
    let mut args = Vec::new();
    if filter_blobs {
        args.extend(strings(&["-c", "protocol.version=2"]));
    }
    args.extend(strings(&["clone", "--no-checkout", "--depth=1"]));
    if sparse {
        if filter_blobs {
            args.push("--filter=blob:none".into());
        }
        args.push("--sparse".into());
    }
    args.extend(strings(&["--progress", "--branch"]));
    args.push(branch.into());
    args.push("--".into());
    args.push(remote.into());
    args.push(directory.as_os_str().to_owned());
    args
}

fn can_retry_unfiltered(stderr: &[u8]) -> bool {
    let message = String::from_utf8_lossy(stderr).to_lowercase();
    (message.contains("filter")
        && (message.contains("not support")
            || message.contains("unsupported")
            || message.contains("not recognized")
            || message.contains("does not allow")))
        || (message.contains("protocol version 2")
            && (message.contains("not support")
                || message.contains("unsupported")
                || message.contains("not recognized")))
}

fn is_partial_clone(directory: &Path) -> Result<bool, String> {
    let config = git_output(
        Some(directory),
        &strings(&["config", "--bool", "--get", "remote.origin.promisor"]),
    )?;
    Ok(config.status.success() && String::from_utf8_lossy(&config.stdout).trim() == "true")
}

fn remove_partial_checkout(directory: &Path) -> Result<(), String> {
    match fs::remove_dir_all(directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "could not remove incomplete checkout '{}': {error}",
            directory.display()
        )),
    }
}

fn resolve_revision(
    directory: &Path,
    branch: &str,
    base_sha: &str,
    revision: &str,
) -> Result<String, String> {
    for deepen_by in [32_u32, 128, 512, 2_048, 8_192, 32_768] {
        let revision_sha = resolve_commit(directory, revision);
        let ancestry = match revision_sha {
            Ok(ref sha) => git_output(
                Some(directory),
                &[
                    "merge-base".into(),
                    "--is-ancestor".into(),
                    sha.clone().into(),
                    base_sha.into(),
                ],
            )?,
            Err(_) => {
                if !is_shallow(directory)? {
                    return Err(format!(
                        "revision '{revision}' is not reachable from branch '{branch}'"
                    ));
                }
                deepen_branch(directory, branch, base_sha, deepen_by)?;
                continue;
            }
        };

        if ancestry.status.success() {
            return revision_sha;
        }
        if !is_shallow(directory)? {
            return Err(format!(
                "revision '{revision}' is not reachable from branch '{branch}'"
            ));
        }
        deepen_branch(directory, branch, base_sha, deepen_by)?;
    }

    Err(format!(
        "could not verify that revision '{revision}' is reachable from branch '{branch}' without fetching excessive history"
    ))
}

fn resolve_commit(directory: &Path, revision: &str) -> Result<String, String> {
    let spec = format!("{revision}^{{commit}}");
    let output = git_checked(
        Some(directory),
        vec![
            "rev-parse".into(),
            "--verify".into(),
            "--end-of-options".into(),
            spec.into(),
        ],
        "resolve commit",
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn is_shallow(directory: &Path) -> Result<bool, String> {
    let output = git_checked(
        Some(directory),
        strings(&["rev-parse", "--is-shallow-repository"]),
        "check shallow history",
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim() == "true")
}

fn deepen_branch(
    directory: &Path,
    branch: &str,
    expected_base: &str,
    deepen_by: u32,
) -> Result<(), String> {
    let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    let partial_clone = is_partial_clone(directory)?;
    let mut args = Vec::new();
    if partial_clone {
        args.extend(strings(&["-c", "protocol.version=2"]));
    }
    args.extend(strings(&["fetch", "--quiet"]));
    if partial_clone {
        args.push("--filter=blob:none".into());
    }
    args.push(format!("--deepen={deepen_by}").into());
    args.push("origin".into());
    args.push(refspec.into());
    git_checked(Some(directory), args, "deepen branch history")?;
    let fetched_base = rev_parse(directory, &format!("refs/remotes/origin/{branch}"))?;
    if fetched_base != expected_base {
        return Err(format!(
            "branch '{branch}' changed while resolving the revision; run the command again"
        ));
    }
    Ok(())
}

fn verify_directory(directory: &Path, revision: &str, repo_path: &str) -> Result<(), String> {
    if repo_path == "." {
        return Ok(());
    }
    let literal_path = format!(":(literal){repo_path}");
    let output = git_checked(
        Some(directory),
        vec![
            "ls-tree".into(),
            "-d".into(),
            "--name-only".into(),
            revision.into(),
            "--".into(),
            literal_path.into(),
        ],
        "check repository path",
    )?;
    if String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        return Err(format!(
            "directory '{repo_path}' does not exist at the selected revision"
        ));
    }
    Ok(())
}

fn work_branch_name(repo_path: &str, revision: &str) -> String {
    let slug = if repo_path == "." {
        "root".to_owned()
    } else {
        repo_path
            .split('/')
            .map(safe_slug)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    };
    let suffix = revision.chars().take(8).collect::<String>();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!(
        "got/{}-{suffix}-{timestamp}-{}",
        truncate(&slug, 40),
        std::process::id()
    )
}

fn default_directory_name(remote: &str, repo_path: &str) -> String {
    let repository = remote
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("repository")
        .strip_suffix(".git")
        .unwrap_or_else(|| {
            remote
                .trim_end_matches('/')
                .rsplit(['/', ':'])
                .next()
                .unwrap_or("repository")
        });
    if repo_path == "." {
        safe_slug(repository)
    } else {
        let path = repo_path.replace('/', "-");
        format!("{}-{}", safe_slug(repository), safe_slug(&path))
    }
}

fn safe_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    slug.trim_matches('-').to_owned()
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn set_got_config(directory: &Path, key: &str, value: &str) -> Result<(), String> {
    git_checked(
        Some(directory),
        vec!["config".into(), "--local".into(), key.into(), value.into()],
        "save checkout metadata",
    )?;
    Ok(())
}

fn rev_parse(directory: &Path, reference: &str) -> Result<String, String> {
    let output = git_checked(
        Some(directory),
        vec![
            "rev-parse".into(),
            "--verify".into(),
            "--end-of-options".into(),
            reference.into(),
        ],
        "resolve fetched branch",
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn push_current_branch() -> Result<i32, String> {
    let branch_output = git_checked(
        None,
        strings(&["branch", "--show-current"]),
        "find current branch",
    )?;
    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_owned();
    if branch.is_empty() {
        return Err("cannot push from a detached HEAD; check out a branch first".to_owned());
    }

    let got_remote = git_output(
        None,
        &strings(&["config", "--local", "--get", "got.remote"]),
    )?;
    let upstream = git_output(
        None,
        &strings(&[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ]),
    )?;
    let remote = if upstream.status.success() {
        let configured = git_output(
            None,
            &[
                "config".into(),
                "--get".into(),
                format!("branch.{branch}.remote").into(),
            ],
        )?;
        if !configured.status.success() {
            return Err("could not determine the configured push remote".to_owned());
        }
        String::from_utf8_lossy(&configured.stdout)
            .trim()
            .to_owned()
    } else if got_remote.status.success() {
        String::from_utf8_lossy(&got_remote.stdout)
            .trim()
            .to_owned()
    } else {
        let remotes = git_checked(None, strings(&["remote"]), "list configured remotes")?;
        let remotes = String::from_utf8_lossy(&remotes.stdout)
            .lines()
            .map(str::trim)
            .filter(|remote| !remote.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if remotes.len() != 1 {
            return Err(
                "cannot choose a push remote; configure an upstream or use 'got push <remote>'"
                    .to_owned(),
            );
        }
        remotes[0].clone()
    };

    warn_if_base_moved(&remote)?;

    if upstream.status.success() {
        run_git_passthrough("push", &[])
    } else {
        run_git_passthrough(
            "push",
            &[
                OsString::from("--set-upstream"),
                OsString::from(remote),
                OsString::from("HEAD"),
            ],
        )
    }
}

fn warn_if_base_moved(remote: &str) -> Result<(), String> {
    let branch_output = git_output(
        None,
        &strings(&["config", "--local", "--get", "got.base-branch"]),
    )?;
    let sha_output = git_output(
        None,
        &strings(&["config", "--local", "--get", "got.base-tip-sha"]),
    )?;
    if !branch_output.status.success() || !sha_output.status.success() {
        return Ok(());
    }
    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_owned();
    let base_sha = String::from_utf8_lossy(&sha_output.stdout)
        .trim()
        .to_owned();
    let ref_name = format!("refs/heads/{branch}");
    let output = git_checked(
        None,
        vec![
            "ls-remote".into(),
            "--heads".into(),
            remote.into(),
            ref_name.clone().into(),
        ],
        "check base branch",
    )?;
    match find_ref_sha(&output.stdout, &ref_name) {
        Some(current_sha) if current_sha != base_sha => eprintln!(
            "warning: base branch '{branch}' advanced since checkout; pushing the work branch without rebasing"
        ),
        None => {
            return Err(format!(
                "base branch '{branch}' is no longer visible on the remote; refusing bare push"
            ))
        }
        _ => {}
    }
    Ok(())
}

fn run_git_passthrough(subcommand: &str, args: &[OsString]) -> Result<i32, String> {
    let mut command = Command::new("git");
    command.arg(subcommand).args(args);
    let status = command.status().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            "Git is required and must be available on PATH".to_owned()
        } else {
            format!("could not start Git: {error}")
        }
    })?;
    Ok(exit_code(status))
}

fn git_output(cwd: Option<&Path>, args: &[OsString]) -> Result<Output, String> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.args(args).output().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            "Git is required and must be available on PATH".to_owned()
        } else {
            format!("could not start Git: {error}")
        }
    })
}

fn git_checked(cwd: Option<&Path>, args: Vec<OsString>, action: &str) -> Result<Output, String> {
    let output = git_output(cwd, &args)?;
    if output.status.success() {
        return Ok(output);
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if detail.is_empty() {
        Err(format!("could not {action}"))
    } else {
        Err(format!("could not {action}: {detail}"))
    }
}

fn run_git_stream(args: &[OsString]) -> Result<(ExitStatus, Vec<u8>), String> {
    let mut child = Command::new("git")
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                "Git is required and must be available on PATH".to_owned()
            } else {
                format!("could not start Git: {error}")
            }
        })?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "could not read Git output".to_owned())?;
    let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
        let mut captured = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut output = io::stderr().lock();
        loop {
            let count = stderr.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            captured.extend_from_slice(&buffer[..count]);
            output.write_all(&buffer[..count])?;
            output.flush()?;
        }
        Ok(captured)
    });
    let status = child
        .wait()
        .map_err(|error| format!("could not wait for Git: {error}"))?;
    let captured = reader
        .join()
        .map_err(|_| "failed while reading Git output".to_owned())?
        .map_err(|error| format!("could not read Git output: {error}"))?;
    Ok((status, captured))
}

fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn checkout_requires_remote_and_defaults_to_repository_root() {
        assert!(parse_checkout_args(&[]).is_err());
        let options = parse_checkout_args(&os_args(&["https://github.com/acme/repo.git"])).unwrap();
        assert_eq!(options.path, ".");
        assert_eq!(
            options,
            CheckoutOptions {
                remote: "https://github.com/acme/repo.git".to_owned(),
                path: ".".to_owned(),
                branch: None,
                revision: None,
                directory: None,
            }
        );
    }

    #[test]
    fn parses_checkout_options() {
        let options = parse_checkout_args(&os_args(&[
            "acme/repo",
            "apps/api",
            "--branch",
            "main",
            "--revision",
            "abc123",
            "--directory",
            "checkout",
        ]))
        .unwrap();
        assert_eq!(
            options,
            CheckoutOptions {
                remote: "acme/repo".to_owned(),
                path: "apps/api".to_owned(),
                branch: Some("main".to_owned()),
                revision: Some("abc123".to_owned()),
                directory: Some(PathBuf::from("checkout")),
            }
        );
    }

    #[test]
    fn rejects_parent_traversal_and_windows_separators() {
        assert!(validate_repo_path("../secret").is_err());
        assert!(validate_repo_path("apps\\api").is_err());
        assert!(validate_repo_path("/absolute/path").is_err());
        assert!(validate_repo_path("apps/.git/config").is_err());
        assert!(validate_repo_path("apps//api").is_err());
        assert!(validate_repo_path("apps/.GIT/config").is_err());
        assert!(validate_repo_path("apps/\napi").is_err());
        assert!(validate_repo_path("apps/api").is_ok());
    }

    #[test]
    fn generates_portable_names() {
        assert_eq!(
            default_directory_name("https://github.com/acme/mono.git", "apps/api"),
            "mono-apps-api"
        );
        assert_eq!(
            default_directory_name("https://github.com/acme/mono.git", "."),
            "mono"
        );
        assert_eq!(
            normalize_remote("acme/mono"),
            "https://github.com/acme/mono.git"
        );
    }

    #[test]
    fn parses_remote_refs_exactly() {
        let output =
            b"ref: refs/heads/main\tHEAD\n0123456789012345678901234567890123456789\tHEAD\n";
        assert_eq!(
            find_ref_sha(output, "HEAD"),
            Some("0123456789012345678901234567890123456789".to_owned())
        );
    }
}
