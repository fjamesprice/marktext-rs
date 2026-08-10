//! E2 optional — `accesskit` alone. §12.2 bills it at ~0.5 MB; it belongs to
//! M4 (R6: "Accessibility retrofitted"), but links cheaply today.
use accesskit::{Node, NodeId, Role, Tree, TreeUpdate};

fn main() {
    let root_id = NodeId(0);
    let root = Node::new(Role::Window);
    let update = TreeUpdate {
        nodes: vec![(root_id, root)],
        tree: Some(Tree::new(root_id)),
        focus: root_id,
    };
    println!(
        "[accesskit] built a TreeUpdate: {} node(s), root role={:?}",
        update.nodes.len(),
        update.nodes[0].1.role()
    );
}
