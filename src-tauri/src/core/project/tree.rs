//! Project type detection and the semantic tree (DESIGN.md sections 4.1 and 7.2).
//!
//! M1 classifies by file system conventions (directory roles, file names, a cheap
//! playbook content check). Parsing HCL/YAML for variables, outputs or hosts comes
//! with the schema/parser work in M4.

use std::io::Read;

use cap_std::fs::Dir;
use serde::Serialize;
use ts_rs::TS;

use super::ProjectError;
use super::fs::ProjectFs;

/// Directories that are never shown or descended into.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".terraform",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
];
const MAX_ENTRIES: usize = 50_000;
const MAX_DEPTH: usize = 24;
/// Bytes read to decide whether a YAML file is a playbook.
const PLAYBOOK_SNIFF_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NodeKind {
    Directory,
    File,
    Symlink,
    /// Directory with `.tf` files that is not below a `modules/` directory.
    TerraformRoot,
    /// Directory with `.tf` files below a `modules/` directory.
    TerraformModule,
    TerraformFile,
    TerraformVars,
    TerraformLock,
    AnsibleConfig,
    Playbook,
    RolesDir,
    Role,
    /// `tasks/`, `handlers/`, `templates/`, … inside a role.
    RoleSection,
    Inventory,
    GroupVars,
    HostVars,
    CollectionRequirements,
    ProjectConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TreeNode {
    pub name: String,
    /// Path relative to the project root, `/`-separated; empty for the root.
    pub path: String,
    pub kind: NodeKind,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Detected {
    pub terraform: bool,
    pub ansible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectTree {
    pub root: TreeNode,
    pub detected: Detected,
    /// True if the walk stopped at the entry or depth limit.
    pub truncated: bool,
}

struct Walk {
    entries: usize,
    truncated: bool,
    detected: Detected,
}

pub fn build_tree(fs: &ProjectFs, name: &str) -> Result<ProjectTree, ProjectError> {
    let root_dir = fs.dir_handle().map_err(|e| ProjectError::from_io("", e))?;
    let ansible_project = ["ansible.cfg", "playbooks", "roles", "inventory"]
        .iter()
        .any(|n| root_dir.symlink_metadata(n).is_ok())
        || root_dir
            .symlink_metadata("collections/requirements.yml")
            .is_ok();
    let mut walk = Walk {
        entries: 0,
        truncated: false,
        detected: Detected {
            terraform: false,
            ansible: ansible_project,
        },
    };
    let ctx = Ctx {
        ansible: ansible_project,
        in_modules: false,
        in_roles: false,
        in_role: false,
        below_roles: false,
    };
    let children = walk_dir(&root_dir, "", 0, ctx, &mut walk)?;
    let root = TreeNode {
        name: name.to_owned(),
        path: String::new(),
        kind: if walk.detected.terraform && has_tf_files(&children) {
            NodeKind::TerraformRoot
        } else {
            NodeKind::Directory
        },
        children,
    };
    Ok(ProjectTree {
        root,
        detected: walk.detected,
        truncated: walk.truncated,
    })
}

#[derive(Clone, Copy)]
struct Ctx {
    ansible: bool,
    in_modules: bool,
    in_roles: bool,
    in_role: bool,
    /// Anywhere below `roles/` (task files there are never playbooks).
    below_roles: bool,
}

fn walk_dir(
    dir: &Dir,
    prefix: &str,
    depth: usize,
    ctx: Ctx,
    walk: &mut Walk,
) -> Result<Vec<TreeNode>, ProjectError> {
    let mut nodes = Vec::new();
    let entries = dir
        .entries()
        .map_err(|e| ProjectError::from_io(prefix, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| ProjectError::from_io(prefix, e))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue; // non-UTF-8 names are not shown
        };
        if walk.entries >= MAX_ENTRIES {
            walk.truncated = true;
            break;
        }
        walk.entries += 1;
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry
            .file_type()
            .map_err(|e| ProjectError::from_io(&path, e))?;
        if file_type.is_symlink() {
            // never followed: avoids loops and escapes; content access goes through ProjectFs
            nodes.push(leaf(name, path, NodeKind::Symlink));
        } else if file_type.is_dir() {
            if SKIPPED_DIRS.contains(&name.as_str()) || (path == ".iac-studio/runs") {
                continue;
            }
            if depth + 1 >= MAX_DEPTH {
                walk.truncated = true;
                continue;
            }
            let sub = entry
                .open_dir()
                .map_err(|e| ProjectError::from_io(&path, e))?;
            let child_ctx = Ctx {
                ansible: ctx.ansible,
                in_modules: ctx.in_modules || name == "modules",
                in_roles: depth == 0 && name == "roles",
                in_role: ctx.in_roles,
                below_roles: ctx.below_roles || (depth == 0 && name == "roles"),
            };
            let children = walk_dir(&sub, &path, depth + 1, child_ctx, walk)?;
            let kind = classify_dir(&name, depth, ctx, &children);
            if matches!(kind, NodeKind::TerraformRoot | NodeKind::TerraformModule) {
                walk.detected.terraform = true;
            }
            nodes.push(TreeNode {
                name,
                path,
                kind,
                children,
            });
        } else if file_type.is_file() {
            let kind = classify_file(dir, &name, &path, ctx);
            if kind == NodeKind::TerraformFile && depth == 0 {
                walk.detected.terraform = true;
            }
            nodes.push(leaf(name, path, kind));
        }
    }
    nodes.sort_by(|a, b| {
        let a_dir = !a.children.is_empty() || is_dir_kind(a.kind);
        let b_dir = !b.children.is_empty() || is_dir_kind(b.kind);
        b_dir.cmp(&a_dir).then_with(|| a.name.cmp(&b.name))
    });
    Ok(nodes)
}

fn leaf(name: String, path: String, kind: NodeKind) -> TreeNode {
    TreeNode {
        name,
        path,
        kind,
        children: Vec::new(),
    }
}

fn is_dir_kind(kind: NodeKind) -> bool {
    !matches!(
        kind,
        NodeKind::File
            | NodeKind::Symlink
            | NodeKind::TerraformFile
            | NodeKind::TerraformVars
            | NodeKind::TerraformLock
            | NodeKind::AnsibleConfig
            | NodeKind::Playbook
            | NodeKind::CollectionRequirements
            | NodeKind::ProjectConfig
    )
}

fn has_tf_files(children: &[TreeNode]) -> bool {
    children.iter().any(|c| c.kind == NodeKind::TerraformFile)
}

fn classify_dir(name: &str, depth: usize, ctx: Ctx, children: &[TreeNode]) -> NodeKind {
    if has_tf_files(children) {
        return if ctx.in_modules || name == "modules" {
            NodeKind::TerraformModule
        } else {
            NodeKind::TerraformRoot
        };
    }
    if ctx.in_role {
        return NodeKind::RoleSection;
    }
    if ctx.in_roles {
        return NodeKind::Role;
    }
    match (depth, name) {
        (0, "roles") => NodeKind::RolesDir,
        (_, "group_vars") => NodeKind::GroupVars,
        (_, "host_vars") => NodeKind::HostVars,
        (0, "inventory" | "inventories") if ctx.ansible => NodeKind::Inventory,
        _ => NodeKind::Directory,
    }
}

fn classify_file(dir: &Dir, name: &str, path: &str, ctx: Ctx) -> NodeKind {
    if path == ".iac-studio/project.toml" {
        return NodeKind::ProjectConfig;
    }
    if name.ends_with(".tf") || name.ends_with(".tf.json") {
        return NodeKind::TerraformFile;
    }
    if name.ends_with(".tfvars") || name.ends_with(".tfvars.json") {
        return NodeKind::TerraformVars;
    }
    if name == ".terraform.lock.hcl" {
        return NodeKind::TerraformLock;
    }
    if name == "ansible.cfg" {
        return NodeKind::AnsibleConfig;
    }
    if path == "collections/requirements.yml" || path == "requirements.yml" {
        return NodeKind::CollectionRequirements;
    }
    if ctx.ansible
        && matches!(
            name,
            "hosts" | "hosts.ini" | "inventory.ini" | "inventory.yml"
        )
    {
        return NodeKind::Inventory;
    }
    if ctx.ansible
        && !ctx.below_roles
        && (name.ends_with(".yml") || name.ends_with(".yaml"))
        && looks_like_playbook(dir, name)
    {
        return NodeKind::Playbook;
    }
    NodeKind::File
}

/// Cheap content check: a playbook is a YAML list whose items start plays
/// (`hosts:`) or import other playbooks.
fn looks_like_playbook(dir: &Dir, name: &str) -> bool {
    let Ok(file) = dir.open(name) else {
        return false;
    };
    let mut head = String::new();
    if file
        .take(PLAYBOOK_SNIFF_BYTES)
        .read_to_string(&mut head)
        .is_err()
    {
        return false;
    }
    is_playbook_text(&head)
}

pub(crate) fn is_playbook_text(text: &str) -> bool {
    let mut first_item_seen = false;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" {
            continue;
        }
        if !first_item_seen {
            if !trimmed.starts_with("- ") && trimmed != "-" {
                return false; // top level is not a list
            }
            first_item_seen = true;
        }
        let key = trimmed.trim_start_matches("- ").trim_start();
        let indent = line.len() - line.trim_start().len();
        let is_item_level = trimmed.starts_with("- ") || indent == 2;
        if is_item_level
            && [
                "hosts:",
                "import_playbook:",
                "ansible.builtin.import_playbook:",
            ]
            .iter()
            .any(|k| key.starts_with(k))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use camino::Utf8PathBuf;

    use super::*;

    fn write(root: &Utf8PathBuf, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn find<'a>(node: &'a TreeNode, path: &str) -> Option<&'a TreeNode> {
        if node.path == path {
            return Some(node);
        }
        node.children.iter().find_map(|c| find(c, path))
    }

    fn mixed_repo() -> (tempfile::TempDir, Utf8PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().join("repo")).unwrap();
        write(
            &root,
            "terraform/main.tf",
            "resource \"null_resource\" \"x\" {}\n",
        );
        write(&root, "terraform/prod.tfvars", "name = \"x\"\n");
        write(&root, "terraform/.terraform.lock.hcl", "");
        write(
            &root,
            "terraform/modules/net/main.tf",
            "variable \"x\" {}\n",
        );
        write(&root, "terraform/.terraform/providers/junk", "");
        write(&root, "ansible.cfg", "[defaults]\n");
        write(
            &root,
            "site.yml",
            "---\n- name: Web\n  hosts: web\n  tasks: []\n",
        );
        write(
            &root,
            "playbooks/import.yml",
            "- import_playbook: ../site.yml\n",
        );
        write(&root, "playbooks/vars.yml", "foo: bar\n");
        write(
            &root,
            "roles/nginx/tasks/main.yml",
            "- name: x\n  debug: {}\n",
        );
        write(&root, "roles/nginx/handlers/main.yml", "");
        write(&root, "inventory/hosts.ini", "[web]\nlocalhost\n");
        write(&root, "group_vars/all.yml", "x: 1\n");
        write(&root, "collections/requirements.yml", "collections: []\n");
        write(&root, ".iac-studio/project.toml", "schema = 1\n");
        write(&root, ".iac-studio/runs/r1/events.ndjson", "");
        write(&root, ".git/HEAD", "ref: x\n");
        (tmp, root)
    }

    #[test]
    fn classifies_mixed_repository() {
        let (_tmp, root) = mixed_repo();
        let fs = ProjectFs::open(&root).unwrap();
        let tree = build_tree(&fs, "repo").unwrap();
        assert_eq!(
            tree.detected,
            Detected {
                terraform: true,
                ansible: true
            }
        );
        let r = &tree.root;
        let kind = |p: &str| find(r, p).map(|n| n.kind);
        assert_eq!(kind("terraform"), Some(NodeKind::TerraformRoot));
        assert_eq!(kind("terraform/main.tf"), Some(NodeKind::TerraformFile));
        assert_eq!(kind("terraform/prod.tfvars"), Some(NodeKind::TerraformVars));
        assert_eq!(
            kind("terraform/.terraform.lock.hcl"),
            Some(NodeKind::TerraformLock)
        );
        assert_eq!(
            kind("terraform/modules/net"),
            Some(NodeKind::TerraformModule)
        );
        assert_eq!(kind("terraform/.terraform"), None);
        assert_eq!(kind("ansible.cfg"), Some(NodeKind::AnsibleConfig));
        assert_eq!(kind("site.yml"), Some(NodeKind::Playbook));
        assert_eq!(kind("playbooks/import.yml"), Some(NodeKind::Playbook));
        assert_eq!(kind("playbooks/vars.yml"), Some(NodeKind::File));
        assert_eq!(kind("roles"), Some(NodeKind::RolesDir));
        assert_eq!(kind("roles/nginx"), Some(NodeKind::Role));
        assert_eq!(kind("roles/nginx/tasks"), Some(NodeKind::RoleSection));
        assert_eq!(kind("roles/nginx/tasks/main.yml"), Some(NodeKind::File));
        assert_eq!(kind("inventory"), Some(NodeKind::Inventory));
        assert_eq!(kind("group_vars"), Some(NodeKind::GroupVars));
        assert_eq!(
            kind("collections/requirements.yml"),
            Some(NodeKind::CollectionRequirements)
        );
        assert_eq!(
            kind(".iac-studio/project.toml"),
            Some(NodeKind::ProjectConfig)
        );
        assert_eq!(kind(".iac-studio/runs"), None);
        assert_eq!(kind(".git"), None);
        assert!(!tree.truncated);
    }

    #[test]
    fn plain_terraform_root_and_sorting() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        write(&root, "b.tf", "");
        write(&root, "a.tf", "");
        write(&root, "zdir/x.txt", "");
        let fs = ProjectFs::open(&root).unwrap();
        let tree = build_tree(&fs, "p").unwrap();
        assert_eq!(tree.root.kind, NodeKind::TerraformRoot);
        assert_eq!(
            tree.detected,
            Detected {
                terraform: true,
                ansible: false
            }
        );
        let names: Vec<_> = tree.root.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["zdir", "a.tf", "b.tf"]);
    }

    #[test]
    fn symlinks_are_listed_but_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(tmp.path().join("p")).unwrap();
        write(&root, "a.tf", "");
        std::fs::create_dir_all(tmp.path().join("outside/deep")).unwrap();
        std::fs::write(tmp.path().join("outside/deep/x.tf"), "").unwrap();
        symlink(tmp.path().join("outside"), root.join("out")).unwrap();
        symlink(".", root.join("loop")).unwrap();
        let fs = ProjectFs::open(&root).unwrap();
        let tree = build_tree(&fs, "p").unwrap();
        let out = find(&tree.root, "out").unwrap();
        assert_eq!(out.kind, NodeKind::Symlink);
        assert!(out.children.is_empty());
        assert_eq!(find(&tree.root, "loop").unwrap().kind, NodeKind::Symlink);
    }

    #[test]
    fn playbook_heuristic() {
        assert!(is_playbook_text("- hosts: all\n  tasks: []\n"));
        assert!(is_playbook_text("# c\n---\n- name: x\n  hosts: all\n"));
        assert!(is_playbook_text(
            "- ansible.builtin.import_playbook: a.yml\n"
        ));
        assert!(!is_playbook_text("hosts: all\n"));
        assert!(!is_playbook_text("- name: task\n  debug:\n    hosts: x\n"));
        assert!(!is_playbook_text(""));
    }
}
