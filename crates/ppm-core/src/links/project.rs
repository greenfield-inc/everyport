//! Project and workspace from a working directory: the nearest git root, the
//! nearest manifest, framework, branch, GitHub remote and Vercel link.

use super::watch::Cache;
use super::{env, home, preview};
use crate::platform::ProcDetails;
use crate::protocol::{Project, VercelProject, Workspace, WorkspaceKind};
use std::fs;
use std::path::{Path, PathBuf};

pub const PANE_SESSION_ENV: &str = "PANE_SESSION_ID";
pub const PANE_PANEL_ENV: &str = "PANE_PANEL_ID";
pub const PANE_WORKSPACE_ENV: &str = "PANE_WORKSPACE_PATH";
pub const CONDUCTOR_WORKSPACE_ENV: &str = "CONDUCTOR_WORKSPACE_NAME";

#[derive(Default)]
pub struct Resolver {
    cache: Cache<PathBuf, Resolved>,
}

#[derive(Clone)]
struct Resolved {
    /// Named by `name`, or else by the process.
    project: Project,
    name: Option<String>,
    git: Option<Git>,
}

impl Resolver {
    /// `process_name` names a project that has no usable folder name.
    pub fn project(&self, cwd: &Path, command: Option<&str>, process_name: &str) -> Project {
        let Resolved {
            mut project,
            name,
            git,
        } = self.resolve(cwd);
        project.name = name.unwrap_or_else(|| process_name.to_string());
        if command.is_some_and(runs_storybook) {
            project.framework = Some("Storybook".into());
        }
        if let (Some(vercel), Some(git), Some(repo), Some(branch)) =
            (&mut project.vercel, &git, &project.github, &project.branch)
        {
            vercel.preview_url = preview::preview_url(&git.root, repo, branch);
        }
        project
    }

    pub fn workspace(&self, cwd: &Path, chain: &[ProcDetails]) -> Option<Workspace> {
        let git = self.resolve(cwd).git;
        let folder_name = file_name(git.as_ref().map_or(cwd, |g| &g.root)).unwrap_or_default();

        if let Some(process) = chain
            .iter()
            .find(|p| env(std::slice::from_ref(p), PANE_SESSION_ENV).is_some())
        {
            let one = std::slice::from_ref(process);
            let url = pane_url(env(one, PANE_SESSION_ENV), env(one, PANE_PANEL_ENV));
            let name = env(one, PANE_WORKSPACE_ENV)
                .and_then(|path| path.rsplit(['/', '\\']).find(|s| !s.is_empty()))
                .map_or(folder_name, str::to_string);
            return Some(workspace(WorkspaceKind::Pane, name, url));
        }
        if let Some(name) = env(chain, CONDUCTOR_WORKSPACE_ENV) {
            return Some(workspace(WorkspaceKind::Conductor, name.into(), None));
        }

        // Without the environment, go by the worktree's folder.
        let git = git.filter(|g| g.main_root.is_some())?;
        let parent = git.root.parent();
        let grandparent = parent.and_then(Path::parent);
        // Conductor: ~/conductor/workspaces/<repo>/<name>, or <repo>/.conductor/<name> before that.
        let conductor = parent.and_then(file_name).as_deref() == Some(".conductor")
            || grandparent.is_some_and(|g| {
                file_name(g).as_deref() == Some("workspaces")
                    && g.parent().and_then(file_name).as_deref() == Some("conductor")
            });
        // Pane's default layout: <repo>/worktrees/<name>.
        let pane = git
            .main_root
            .as_ref()
            .is_some_and(|main| parent == Some(&main.join("worktrees")));
        let kind = match (conductor, pane) {
            (true, _) => WorkspaceKind::Conductor,
            (_, true) => WorkspaceKind::Pane,
            _ => WorkspaceKind::GitWorktree,
        };
        Some(workspace(kind, folder_name, None))
    }

    fn resolve(&self, cwd: &Path) -> Resolved {
        self.cache.get(&cwd.to_path_buf(), |_| resolve(cwd))
    }
}

/// The program, or the script a package manager runs, is `storybook`:
/// `storybook dev`, `node node_modules/.bin/storybook dev`, `npx storybook dev`
/// or `npm run storybook`. A config path that merely mentions it doesn't count.
fn runs_storybook(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let skip_run = matches!(words.get(1), Some(&("run" | "exec" | "dlx" | "x")));
    let candidates = if skip_run { [0, 2] } else { [0, 1] };
    candidates.iter().filter_map(|&i| words.get(i)).any(|word| {
        let file = word.rsplit(['/', '\\']).next().unwrap_or(word);
        file.split('.').next() == Some("storybook")
    })
}

fn workspace(kind: WorkspaceKind, name: String, open_url: Option<String>) -> Workspace {
    Workspace {
        kind,
        name,
        open_url,
    }
}

/// `pane://open?pane=<id>&panel=<id>`, which the Pane app opens. Ids Pane
/// would reject leave no link, or no panel.
fn pane_url(pane: Option<&str>, panel: Option<&str>) -> Option<String> {
    let pane = pane.filter(|id| is_link_id(id))?;
    Some(match panel.filter(|id| is_link_id(id)) {
        Some(panel) => format!("pane://open?pane={pane}&panel={panel}"),
        None => format!("pane://open?pane={pane}"),
    })
}

/// Pane accepts ids matching `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$`.
fn is_link_id(id: &str) -> bool {
    id.len() <= 128
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Package managers and the OS install here. Their own git repos (Homebrew
/// is one) and folders never name a server's project.
const SYSTEM_PREFIXES: &[&str] = &[
    "/opt/homebrew",
    "/usr/local/Homebrew",
    "/usr/local",
    "/usr",
    "/nix",
    "/opt/local",
    "/home/linuxbrew/.linuxbrew",
];

/// Walks up from `cwd` to the nearest git root (or the home folder or a
/// system prefix), taking the nearest manifest on the way.
fn resolve(cwd: &Path) -> (Vec<PathBuf>, Resolved) {
    let home = home();
    let in_system = SYSTEM_PREFIXES.iter().any(|p| cwd.starts_with(p));
    let mut manifest: Option<(PathBuf, Manifest)> = None;
    let mut git = None;
    for dir in cwd.ancestors() {
        if home.as_deref() == Some(dir) || SYSTEM_PREFIXES.iter().any(|p| dir == Path::new(p)) {
            break;
        }
        if manifest.is_none() {
            manifest = Manifest::read(dir).map(|m| (dir.to_path_buf(), m));
        }
        git = Git::find(dir);
        if git.is_some() {
            break;
        }
    }

    let mut files = Vec::new();
    let mut vercel_candidates = Vec::new();
    if let Some((dir, m)) = &manifest {
        files.extend(m.files.iter().cloned());
        vercel_candidates.push(dir.join(".vercel").join("project.json"));
    }
    let (mut branch, mut github) = (None, None);
    if let Some(git) = &git {
        files.extend([git.head.clone(), git.config.clone()]);
        vercel_candidates.push(git.root.join(".vercel").join("project.json"));
        branch = fs::read_to_string(&git.head)
            .ok()
            .and_then(|h| branch_from_head(&h));
        github = fs::read_to_string(&git.config)
            .ok()
            .and_then(|c| github_from_config(&c));
    }
    vercel_candidates.dedup();
    let vercel = vercel_candidates.iter().find_map(|path| {
        let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
        Some(VercelProject {
            project_id: json.get("projectId")?.as_str()?.to_string(),
            preview_url: None,
        })
    });
    files.extend(vercel_candidates);

    let repo_folder = git
        .as_ref()
        .map(|g| g.main_root.as_deref().unwrap_or(&g.root));
    let name = manifest
        .as_ref()
        .and_then(|(_, m)| m.name.clone())
        .or_else(|| repo_folder.and_then(file_name))
        .or_else(|| file_name(cwd).filter(|_| !in_system));
    let project = Project {
        name: String::new(),
        root: git
            .as_ref()
            .map(|g| g.root.as_path())
            .or(manifest.as_ref().map(|(dir, _)| dir.as_path()))
            .map(|p| p.to_string_lossy().into_owned()),
        framework: manifest.and_then(|(_, m)| m.framework),
        branch,
        worktree: git
            .as_ref()
            .filter(|g| g.main_root.is_some())
            .and_then(|g| file_name(&g.root)),
        github,
        vercel,
    };
    (files, Resolved { project, name, git })
}

// ---------------------------------------------------------------- git

#[derive(Clone)]
struct Git {
    /// The working tree: the folder holding `.git`.
    root: PathBuf,
    /// For a linked worktree, the main working tree it was added from.
    main_root: Option<PathBuf>,
    head: PathBuf,
    config: PathBuf,
}

impl Git {
    fn find(dir: &Path) -> Option<Git> {
        let dot_git = dir.join(".git");
        let meta = fs::metadata(&dot_git).ok()?;
        let (git_dir, common_dir) = if meta.is_dir() {
            (dot_git.clone(), dot_git)
        } else {
            // Worktrees and submodules have a `.git` file pointing at the real git dir.
            let text = fs::read_to_string(&dot_git).ok()?;
            // Git writes `/` here even on Windows; normalize for native separators.
            let git_dir =
                normalize(&dir.join(text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim()));
            // Only linked worktrees have `commondir`; it points back at the main `.git`.
            let common_dir = fs::read_to_string(git_dir.join("commondir"))
                .map_or_else(|_| git_dir.clone(), |c| git_dir.join(c.trim()));
            (git_dir, common_dir)
        };
        let common_dir = normalize(&common_dir);
        let is_worktree = git_dir.join("commondir").exists();
        Some(Git {
            root: dir.to_path_buf(),
            main_root: is_worktree
                .then(|| common_dir.parent().map(Path::to_path_buf))
                .flatten(),
            head: git_dir.join("HEAD"),
            config: common_dir.join("config"),
        })
    }
}

/// Resolves `..` in `<repo>/.git/worktrees/<name>/../..` and uses the OS
/// separator throughout, so paths match file events. `canonicalize` would
/// turn Windows paths into `\\?\C:\...`.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            part => out.push(part),
        }
    }
    out
}

/// `ref: refs/heads/<branch>`, or the short commit for a detached HEAD.
fn branch_from_head(head: &str) -> Option<String> {
    let head = head.trim();
    match head.strip_prefix("ref: refs/heads/") {
        Some(branch) => Some(branch.to_string()),
        None if head.is_empty() => None,
        None => Some(head.chars().take(7).collect()),
    }
}

/// `owner/repo` from the `origin` remote, or else the first remote, on github.com.
fn github_from_config(config: &str) -> Option<String> {
    let mut remotes: Vec<(String, String)> = Vec::new();
    let mut section = None;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line
                .strip_prefix("[remote \"")
                .and_then(|r| r.strip_suffix("\"]"))
                .map(str::to_string);
        } else if let (Some(name), Some(url)) = (&section, line.strip_prefix("url")) {
            if let Some(url) = url.trim_start().strip_prefix('=') {
                remotes.push((name.clone(), url.trim().to_string()));
            }
        }
    }
    let url = remotes
        .iter()
        .find(|(name, _)| name == "origin")
        .or(remotes.first())
        .map(|(_, url)| url.as_str())?;
    let path = [
        "git@github.com:",
        "https://github.com/",
        "ssh://git@github.com/",
        "git://github.com/",
    ]
    .iter()
    .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    (!owner.is_empty() && !repo.is_empty() && !repo.contains('/'))
        .then(|| format!("{owner}/{repo}"))
}

// ---------------------------------------------------------------- manifests

struct Manifest {
    /// The manifest, and any config file the framework was read from.
    files: Vec<PathBuf>,
    name: Option<String>,
    framework: Option<String>,
}

impl Manifest {
    fn read(dir: &Path) -> Option<Manifest> {
        let read = |name: &str| {
            let file = dir.join(name);
            fs::read_to_string(&file).ok().map(|text| (file, text))
        };
        let manifest = |files, name, framework: Option<&str>| {
            Some(Manifest {
                files,
                name,
                framework: framework.map(str::to_string),
            })
        };

        if let Some((file, text)) = read("package.json") {
            let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let has = |dep: &str| {
                ["dependencies", "devDependencies"]
                    .iter()
                    .any(|k| json[k].get(dep).is_some())
            };
            let framework = JS_FRAMEWORKS
                .iter()
                .find(|(dep, _)| has(dep))
                .map(|(_, label)| *label);
            let name = json["name"].as_str().map(str::to_string);
            return manifest(vec![file], name, framework);
        }
        if let Some((file, text)) = read("pyproject.toml").or_else(|| read("requirements.txt")) {
            let name = toml_name(&text, "project").or_else(|| toml_name(&text, "tool.poetry"));
            let text = text.to_lowercase();
            let manage = dir.join("manage.py");
            let framework = PYTHON_FRAMEWORKS
                .iter()
                .find(|(dep, _)| text.contains(dep))
                .map(|(_, label)| *label)
                .or(manage.exists().then_some("Django"))
                .unwrap_or("Python");
            return manifest(vec![file, manage], name, Some(framework));
        }
        if let Some((file, text)) = read("Cargo.toml") {
            return manifest(vec![file], toml_name(&text, "package"), Some("Rust"));
        }
        if let Some((file, text)) = read("go.mod") {
            let module = text.lines().find_map(|l| l.strip_prefix("module "));
            let name = module
                .and_then(|m| m.trim().rsplit('/').next())
                .map(str::to_string);
            return manifest(vec![file], name, Some("Go"));
        }
        if let Some((file, _)) = read("Gemfile") {
            let application = dir.join("config").join("application.rb");
            let rails = application.exists();
            return manifest(
                vec![file, application],
                None,
                Some(if rails { "Rails" } else { "Ruby" }),
            );
        }
        if let Some((file, text)) = read("mix.exs") {
            let name = text
                .split_once("app: :")
                .and_then(|(_, rest)| {
                    rest.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .next()
                })
                .map(str::to_string);
            let phoenix = text.contains(":phoenix");
            return manifest(
                vec![file],
                name,
                Some(if phoenix { "Phoenix" } else { "Elixir" }),
            );
        }
        None
    }
}

/// First match wins, so meta-frameworks come before the tools they build on.
const JS_FRAMEWORKS: &[(&str, &str)] = &[
    ("next", "Next.js"),
    ("nuxt", "Nuxt"),
    ("@remix-run/dev", "Remix"),
    ("@react-router/dev", "React Router"),
    ("astro", "Astro"),
    ("@sveltejs/kit", "SvelteKit"),
    ("expo", "Expo"),
    ("@angular/core", "Angular"),
    ("gatsby", "Gatsby"),
    ("vite", "Vite"),
    ("react-scripts", "Create React App"),
    ("@nestjs/core", "NestJS"),
    ("hono", "Hono"),
    ("fastify", "Fastify"),
    ("express", "Express"),
];

const PYTHON_FRAMEWORKS: &[(&str, &str)] = &[
    ("django", "Django"),
    ("fastapi", "FastAPI"),
    ("flask", "Flask"),
];

/// `name = "..."` inside `[section]`.
fn toml_name(text: &str, section: &str) -> Option<String> {
    let header = format!("[{section}]");
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
        } else if inside {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "name" {
                    return Some(value.trim().trim_matches(['"', '\'']).to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{details, eventually, s, TempDir};
    use super::*;

    /// A repo at `<tmp>/app` on `main` with a GitHub remote, as `git init` and
    /// `git remote add` would leave it.
    fn repo(tmp: &TempDir) -> PathBuf {
        tmp.write("app/.git/HEAD", "ref: refs/heads/main\n");
        tmp.write(
            "app/.git/config",
            "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@github.com:greenfield-inc/port-process-manager.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n",
        );
        tmp.path("app")
    }

    /// A linked worktree at `<tmp>/<rel>` on `branch`, as `git worktree add` would leave it.
    fn worktree(tmp: &TempDir, rel: &str, branch: &str) -> PathBuf {
        let name = rel.rsplit('/').next().unwrap();
        let git_dir = tmp.path(&format!("app/.git/worktrees/{name}"));
        tmp.write(
            &format!("app/.git/worktrees/{name}/HEAD"),
            &format!("ref: refs/heads/{branch}\n"),
        );
        tmp.write(&format!("app/.git/worktrees/{name}/commondir"), "../..\n");
        tmp.write(
            &format!("{rel}/.git"),
            &format!("gitdir: {}\n", s(&git_dir)),
        );
        tmp.path(rel)
    }

    #[test]
    fn next_app_in_a_repo() {
        let tmp = TempDir::new();
        let root = repo(&tmp);
        tmp.write(
            "app/package.json",
            r#"{"name": "greenfield.to", "dependencies": {"next": "15.1.0", "react": "19.0.0"}, "devDependencies": {"vite": "6.0.0"}}"#,
        );
        tmp.write(
            "app/.vercel/project.json",
            r#"{"projectId": "prj_ppm", "orgId": "team_1"}"#,
        );
        let cwd = tmp.write("app/src/.keep", "");

        let project = Resolver::default().project(cwd.parent().unwrap(), Some("next dev"), "node");
        assert_eq!(
            project,
            Project {
                name: "greenfield.to".into(),
                root: Some(s(&root)),
                framework: Some("Next.js".into()),
                branch: Some("main".into()),
                worktree: None,
                github: Some("greenfield-inc/port-process-manager".into()),
                vercel: Some(VercelProject {
                    project_id: "prj_ppm".into(),
                    preview_url: None
                }),
            }
        );
    }

    #[test]
    fn storybook_is_told_apart_by_its_command() {
        let tmp = TempDir::new();
        repo(&tmp);
        tmp.write(
            "app/package.json",
            r#"{"dependencies": {"next": "15.1.0", "vite": "6.0.0"}, "devDependencies": {"storybook": "8.4.0"}}"#,
        );
        let resolver = Resolver::default();
        let cases = [
            ("next dev", "Next.js"),
            ("node node_modules/.bin/storybook dev -p 6006", "Storybook"),
            ("npx storybook dev", "Storybook"),
            ("npm run storybook", "Storybook"),
            (
                "C:\\app\\node_modules\\.bin\\storybook.cmd dev",
                "Storybook",
            ),
            ("vite --config vite.storybook.config.ts", "Next.js"),
        ];
        for (command, framework) in cases {
            let project = resolver.project(&tmp.path("app"), Some(command), "node");
            assert_eq!(project.framework.as_deref(), Some(framework), "{command}");
        }
        // No manifest name, so the repo folder names it.
        assert_eq!(resolver.project(&tmp.path("app"), None, "node").name, "app");
    }

    /// Homebrew's Postgres runs in `/opt/homebrew/var/postgresql@15`, and
    /// `/opt/homebrew` is itself a git repo on the `stable` branch.
    #[test]
    fn the_filesystem_root_and_system_folders_are_named_by_the_process() {
        let resolver = Resolver::default();
        for cwd in [
            "/",
            "/opt/homebrew/var/postgresql@15",
            "/usr/local/var/postgres",
            "/home/linuxbrew/.linuxbrew/var/postgres",
        ] {
            let project = resolver.project(Path::new(cwd), None, "postgres");
            assert_eq!(
                (project.name.as_str(), project.root, project.branch),
                ("postgres", None, None),
                "{cwd}"
            );
        }
    }

    #[test]
    fn manifests_name_the_project_and_its_framework() {
        let cases = [
            ("pyproject.toml", "[project]\nname = \"api\"\ndependencies = [\"fastapi>=0.110\", \"uvicorn\"]\n", "api", "FastAPI"),
            ("Cargo.toml", "[package]\nname = \"ppm-core\"\nversion = \"0.1.0\"\n\n[dependencies]\nname = \"x\"\n", "ppm-core", "Rust"),
            ("go.mod", "module github.com/acme/billing\n\ngo 1.22\n", "billing", "Go"),
            ("mix.exs", "def project do\n  [app: :chat_web, deps: [{:phoenix, \"~> 1.7\"}]]\n", "chat_web", "Phoenix"),
        ];
        for (file, text, name, framework) in cases {
            let tmp = TempDir::new();
            repo(&tmp);
            tmp.write(&format!("app/{file}"), text);
            let project = Resolver::default().project(&tmp.path("app"), None, "node");
            assert_eq!(
                (project.name.as_str(), project.framework.as_deref()),
                (name, Some(framework)),
                "{file}"
            );
        }
    }

    #[test]
    fn config_files_mark_rails_and_django() {
        let tmp = TempDir::new();
        repo(&tmp);
        tmp.write("app/Gemfile", "source \"https://rubygems.org\"\n");
        tmp.write("app/config/application.rb", "");
        tmp.write("app/site/requirements.txt", "psycopg\n");
        tmp.write("app/site/manage.py", "");
        let resolver = Resolver::default();
        assert_eq!(
            resolver
                .project(&tmp.path("app"), None, "node")
                .framework
                .as_deref(),
            Some("Rails")
        );
        assert_eq!(
            resolver
                .project(&tmp.path("app/site"), None, "node")
                .framework
                .as_deref(),
            Some("Django")
        );
    }

    #[test]
    fn worktree_reads_its_own_head_and_the_shared_config() {
        let tmp = TempDir::new();
        repo(&tmp);
        let root = worktree(&tmp, "trees/providence", "menubar-port-monitor");

        let project = Resolver::default().project(&root, None, "node");
        assert_eq!(project.root, Some(s(&root)));
        assert_eq!(project.branch.as_deref(), Some("menubar-port-monitor"));
        assert_eq!(project.worktree.as_deref(), Some("providence"));
        assert_eq!(
            project.github.as_deref(),
            Some("greenfield-inc/port-process-manager")
        );
        // Named after the repo, not the worktree folder.
        assert_eq!(project.name, "app");
    }

    #[test]
    fn detached_head_shows_the_short_commit() {
        let tmp = TempDir::new();
        repo(&tmp);
        tmp.write(
            "app/.git/HEAD",
            "c34cbbcad31977b5c1eb75ded1fae6d08cdd286f\n",
        );
        let project = Resolver::default().project(&tmp.path("app"), None, "node");
        assert_eq!(project.branch.as_deref(), Some("c34cbbc"));
    }

    #[test]
    fn a_branch_switch_is_seen_after_the_file_event() {
        let tmp = TempDir::new();
        repo(&tmp);
        let resolver = Resolver::default();
        let branch = || resolver.project(&tmp.path("app"), None, "node").branch;
        assert_eq!(branch().as_deref(), Some("main"));

        // What `git switch` does: write HEAD.lock, then rename it over HEAD.
        let lock = tmp.write("app/.git/HEAD.lock", "ref: refs/heads/feature/login\n");
        fs::rename(lock, tmp.path("app/.git/HEAD")).unwrap();
        eventually(|| branch().filter(|b| b == "feature/login"));
    }

    #[test]
    fn github_owner_and_repo_from_remote_urls() {
        let remote = |name: &str, url: &str| format!("[remote \"{name}\"]\n\turl = {url}\n");
        let cases = [
            (
                remote("origin", "git@github.com:greenfield-inc/Pane.git"),
                Some("greenfield-inc/Pane"),
            ),
            (
                remote("origin", "https://github.com/greenfield-inc/Pane"),
                Some("greenfield-inc/Pane"),
            ),
            (
                remote("origin", "https://github.com/greenfield-inc/Pane.git/"),
                Some("greenfield-inc/Pane"),
            ),
            (
                remote("origin", "ssh://git@github.com/greenfield-inc/Pane.git"),
                Some("greenfield-inc/Pane"),
            ),
            (
                remote("origin", "git@gitlab.com:greenfield-inc/Pane.git"),
                None,
            ),
            (
                remote("upstream", "https://github.com/tomjohndesign/what-the-port"),
                Some("tomjohndesign/what-the-port"),
            ),
            (
                remote("upstream", "https://github.com/tomjohndesign/what-the-port")
                    + &remote(
                        "origin",
                        "git@github.com:greenfield-inc/port-process-manager.git",
                    ),
                Some("greenfield-inc/port-process-manager"),
            ),
        ];
        for (config, github) in cases {
            let tmp = TempDir::new();
            repo(&tmp);
            tmp.write(
                "app/.git/config",
                &format!("[core]\n\tbare = false\n{config}"),
            );
            let project = Resolver::default().project(&tmp.path("app"), None, "node");
            assert_eq!(project.github.as_deref(), github, "{config}");
        }
    }

    #[test]
    fn pane_terminal_links_back_to_its_panel() {
        let tmp = TempDir::new();
        // The server runs in a scratch folder; Pane's environment still names the workspace.
        let cwd = tmp.write("scratch/.keep", "");
        let chain = [
            details(None, &[]),
            details(
                None,
                &[
                    ("PANE_SESSION_ID", "3f2a9c1e-77b0"),
                    ("PANE_PANEL_ID", "panel-8d41"),
                    ("PANE_WORKSPACE_PATH", "/Users/dev/Pane/worktrees/ppm-links"),
                ],
            ),
        ];
        assert_eq!(
            Resolver::default().workspace(cwd.parent().unwrap(), &chain),
            Some(workspace(
                WorkspaceKind::Pane,
                "ppm-links".into(),
                Some("pane://open?pane=3f2a9c1e-77b0&panel=panel-8d41".into())
            ))
        );
    }

    #[test]
    fn pane_ids_that_pane_would_reject_leave_no_link() {
        let tmp = TempDir::new();
        let chain = [details(
            None,
            &[("PANE_SESSION_ID", "abc&panel=x"), ("PANE_PANEL_ID", "p1")],
        )];
        let found = Resolver::default().workspace(&tmp.0, &chain).unwrap();
        assert_eq!((found.kind, found.open_url), (WorkspaceKind::Pane, None));
    }

    #[test]
    fn conductor_from_its_environment() {
        let tmp = TempDir::new();
        let chain = [details(None, &[("CONDUCTOR_WORKSPACE_NAME", "providence")])];
        assert_eq!(
            Resolver::default().workspace(&tmp.0, &chain),
            Some(workspace(
                WorkspaceKind::Conductor,
                "providence".into(),
                None
            ))
        );
    }

    #[test]
    fn workspace_from_folder_layout_without_environment() {
        let tmp = TempDir::new();
        repo(&tmp);
        let cases = [
            (
                "conductor/workspaces/app/lisbon",
                WorkspaceKind::Conductor,
                "lisbon",
            ),
            (
                "app/.conductor/providence",
                WorkspaceKind::Conductor,
                "providence",
            ),
            ("app/worktrees/ppm-ui", WorkspaceKind::Pane, "ppm-ui"),
            ("trees/fix-login", WorkspaceKind::GitWorktree, "fix-login"),
        ];
        let resolver = Resolver::default();
        for (rel, kind, name) in cases {
            let root = worktree(&tmp, rel, "b");
            assert_eq!(
                resolver.workspace(&root, &[]),
                Some(workspace(kind, name.into(), None)),
                "{rel}"
            );
        }
        assert_eq!(resolver.workspace(&tmp.path("app"), &[]), None);
    }
}
