mod common;

use rbx_loren::ctx::Tool;

#[test]
fn make_project_builds_a_scaffold() {
    let (_dir, root) = common::make_project(Tool::Rojo, "game");
    assert!(root.join("default.project.json").is_file());
    assert!(common::list_files(&root)
        .iter()
        .any(|f| f == "default.project.json"));
}
