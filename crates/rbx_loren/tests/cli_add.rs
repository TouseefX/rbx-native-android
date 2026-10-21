mod common;

use std::cell::RefCell;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use common::{
    list_files, make_project, read, template_dir, tmp_dir, CliOut, Event, FakeRunner, MockLanes,
};
use rbx_loren::cli::{main, Overrides};
use rbx_loren::ctx::{Fetch, Tool};
use rbx_loren::log::Log;
use rbx_loren::names::{derive_package_name, parse_repo_spec};

// A degit stand-in: records the spec and writes `files` into the clone target (no network).
struct FakeFetch {
    files: Vec<(&'static str, &'static str)>,
    fail: Option<&'static str>,
    calls: RefCell<Vec<(String, PathBuf)>>,
}

fn fake_fetch() -> FakeFetch {
    FakeFetch {
        files: vec![
            (
                "default.project.json",
                "{\"name\":\"x\",\"tree\":{\"$path\":\"src\"}}",
            ),
            ("src/init.luau", "return {}"),
        ],
        fail: None,
        calls: RefCell::new(Vec::new()),
    }
}

impl Fetch for FakeFetch {
    fn clone_repo(&self, spec: &str, dest: &Path) -> Result<(), String> {
        self.calls
            .borrow_mut()
            .push((spec.to_string(), dest.to_path_buf()));
        if let Some(fail) = self.fail {
            return Err(fail.to_string());
        }
        for (rel, text) in &self.files {
            let p = dest.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        Ok(())
    }
}

fn run_add(args: &[&str], cwd: &Path, fetch: &FakeFetch, lanes: &MockLanes) -> CliOut {
    let log = Log::memory();
    let run = FakeRunner::new();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let code = main(
        &args,
        Overrides {
            cwd: Some(cwd.to_path_buf()),
            log: Some(&log),
            is_tty: Some(false),
            input: Some(Box::new(Cursor::new(Vec::new()))),
            output: Some(Box::new(std::io::sink())),
            run: Some(&run),
            lanes: Some(lanes),
            fetch: Some(fetch),
            template_dir: Some(template_dir()),
        },
    );
    CliOut {
        code,
        out: log.stdout(),
        err: log.stderr(),
        all: log.output(),
    }
}

#[test]
fn parse_repo_spec_accepts_user_repo_refs_and_github_urls() {
    let cases = [
        ("user/repo", "user", "repo", None),
        ("user/repo#v1.2.0", "user", "repo", Some("v1.2.0")),
        ("https://github.com/user/repo", "user", "repo", None),
        ("https://github.com/user/repo.git", "user", "repo", None),
        ("https://github.com/user/repo/", "user", "repo", None),
        ("http://www.github.com/user/repo", "user", "repo", None),
        (
            "https://github.com/user/repo/tree/feature/x",
            "user",
            "repo",
            Some("feature/x"),
        ),
        (
            "https://github.com/user/repo#dev",
            "user",
            "repo",
            Some("dev"),
        ),
        ("git@github.com:user/repo.git", "user", "repo", None),
        ("github:user/repo", "user", "repo", None),
        ("  Some-Org/my.repo_1  ", "Some-Org", "my.repo_1", None),
    ];
    for (input, owner, repo, git_ref) in cases {
        let r = parse_repo_spec(input).unwrap_or_else(|e| panic!("{input}: {e}"));
        assert_eq!(r.owner, owner, "{input}");
        assert_eq!(r.repo, repo, "{input}");
        assert_eq!(r.git_ref.as_deref(), git_ref, "{input}");
        let expected = match git_ref {
            Some(g) => format!("{owner}/{repo}#{g}"),
            None => format!("{owner}/{repo}"),
        };
        assert_eq!(r.spec, expected);
    }
}

#[test]
fn parse_repo_spec_rejects_anything_that_is_not_a_github_repository() {
    for bad in [
        "",
        "   ",
        "user",
        "https://github.com/",
        "https://gitlab.com/user/repo",
        "user/repo/extra",
        "us er/repo",
        "user/..",
        "user/repo#",
        "user/repo#../x",
        "-user/repo",
        "file:///etc/passwd",
    ] {
        assert!(parse_repo_spec(bad).is_err(), "accepted {bad:?}");
    }
}

#[test]
fn derive_package_name_turns_repo_names_into_safe_identifiers() {
    let ok = |repo: &str| derive_package_name(repo).unwrap();
    assert_eq!(ok("Janitor"), "Janitor");
    assert_eq!(ok("roblox-lua-promise"), "promise");
    assert_eq!(ok("my-lib"), "MyLib");
    assert_eq!(ok("rbx.util"), "RbxUtil");
    assert_eq!(ok("3d-math"), "_3dMath");
    assert_eq!(ok("end"), "End");
}

#[test]
fn add_downloads_into_loren_packages_name_and_refreshes_types_then_sourcemap() {
    let (_dir, root) = make_project(Tool::Rojo, "proj");
    let fetch = fake_fetch();
    let lanes = MockLanes::new(Tool::Rojo);
    let r = run_add(
        &["add", "https://github.com/howmanysmall/Janitor.git"],
        &root,
        &fetch,
        &lanes,
    );
    assert_eq!(r.code, 0, "{}", r.all);
    let calls = fetch.calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "howmanysmall/Janitor");
    assert!(
        !calls[0].1.starts_with(&root),
        "clones into a temp folder first"
    );
    assert!(root.join("loren_packages/Janitor/src/init.luau").exists());
    assert!(r.out.contains("LorenPackages.Janitor"), "{}", r.out);
    let kinds: Vec<&str> = lanes
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::GenerateTypes { .. } => Some("generate_types"),
            Event::Sourcemap { .. } => Some("sourcemap"),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, ["generate_types", "sourcemap"]);
    assert!(!calls[0].1.exists(), "temp folder removed");
}

#[test]
fn add_uses_the_alias_validates_it_and_keeps_refs() {
    let (_dir, root) = make_project(Tool::Rojo, "proj");
    let fetch = fake_fetch();
    let lanes = MockLanes::new(Tool::Rojo);
    let r = run_add(
        &["add", "user/some-lib#v2.0.0", "Lib"],
        &root,
        &fetch,
        &lanes,
    );
    assert_eq!(r.code, 0, "{}", r.all);
    assert_eq!(fetch.calls.borrow()[0].0, "user/some-lib#v2.0.0");
    assert!(root.join("loren_packages/Lib").exists());

    for bad in ["../x", "a/b", "my-lib", "1x", "nil", "CON"] {
        let b = run_add(&["add", "user/other", bad], &root, &fetch, &lanes);
        assert_eq!(b.code, 1, "{bad}");
    }
    assert_eq!(fetch.calls.borrow().len(), 1, "no download for a bad alias");
}

#[test]
fn add_never_replaces_the_bundled_promise_or_an_existing_package_without_force() {
    let (_dir, root) = make_project(Tool::Rojo, "proj");
    let promise_before = list_files(&root.join("loren_packages/Promise"));
    let fetch = fake_fetch();
    let lanes = MockLanes::new(Tool::Rojo);

    let promise = run_add(&["add", "evaera/roblox-lua-promise"], &root, &fetch, &lanes);
    assert_eq!(promise.code, 1);
    assert!(
        promise.err.contains("bundled with Loren"),
        "{}",
        promise.err
    );
    let promise_alias = run_add(&["add", "user/repo", "PROMISE"], &root, &fetch, &lanes);
    assert_eq!(promise_alias.code, 1);
    assert_eq!(fetch.calls.borrow().len(), 0);
    assert_eq!(
        list_files(&root.join("loren_packages/Promise")),
        promise_before
    );

    assert_eq!(
        run_add(&["add", "user/Thing"], &root, &fetch, &lanes).code,
        0
    );
    let dup = run_add(&["add", "other/thing"], &root, &fetch, &lanes);
    assert_eq!(dup.code, 1, "case-insensitive collision");
    assert!(dup.err.contains("already exists"), "{}", dup.err);

    fs::write(root.join("loren_packages/Thing/marker.txt"), "old").unwrap();
    let forced = run_add(
        &["add", "other/thing", "Thing", "--force"],
        &root,
        &fetch,
        &lanes,
    );
    assert_eq!(forced.code, 0, "{}", forced.all);
    assert!(
        !root.join("loren_packages/Thing/marker.txt").exists(),
        "replaced, not merged"
    );
    let backup = list_files(&root.join(".loren-backup"));
    assert!(
        backup
            .iter()
            .any(|f| f.ends_with("loren_packages/Thing/marker.txt")),
        "{backup:?}"
    );
}

#[test]
fn add_leaves_nothing_behind_when_the_download_fails_or_is_empty() {
    let (_dir, root) = make_project(Tool::Rojo, "proj");
    let names = || {
        let mut v: Vec<String> = fs::read_dir(root.join("loren_packages"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    let before = names();
    let lanes = MockLanes::new(Tool::Rojo);
    let failing = FakeFetch {
        fail: Some("could not find commit hash"),
        ..fake_fetch()
    };
    let failed = run_add(&["add", "user/repo"], &root, &failing, &lanes);
    assert_eq!(failed.code, 1);
    assert!(
        failed.err.contains("Could not download user/repo"),
        "{}",
        failed.err
    );
    let empty_fetch = FakeFetch {
        files: Vec::new(),
        ..fake_fetch()
    };
    let empty = run_add(&["add", "user/repo"], &root, &empty_fetch, &lanes);
    assert_eq!(empty.code, 1);
    assert_eq!(names(), before);
}

#[test]
fn add_warns_when_the_package_has_no_entry_point_and_rejects_bad_input_and_non_projects() {
    let (_dir, root) = make_project(Tool::Rojo, "proj");
    let lanes = MockLanes::new(Tool::Rojo);
    let docs = FakeFetch {
        files: vec![("README.md", "# hi")],
        ..fake_fetch()
    };
    let r = run_add(&["add", "user/docs-only"], &root, &docs, &lanes);
    assert_eq!(r.code, 0, "{}", r.all);
    assert!(
        r.err.contains("no default.project.json or init.lua(u)"),
        "{}",
        r.err
    );

    let bad = run_add(
        &["add", "https://gitlab.com/a/b"],
        &root,
        &fake_fetch(),
        &lanes,
    );
    assert_eq!(bad.code, 1);
    let outside_dir = tmp_dir();
    let outside = run_add(
        &["add", "user/repo"],
        outside_dir.path(),
        &fake_fetch(),
        &lanes,
    );
    assert_eq!(outside.code, 1);
    assert!(
        outside.err.contains("No default.project.json"),
        "{}",
        outside.err
    );
    assert!(!read(&root.join("loren_packages/Promise/lib/init.lua")).is_empty());
}
