//! Additional tests for the view tree.

#[cfg(test)]
mod tests {
    use crate::ViewTree;
    use repose_core::{Color, Modifier, TextOverflow, View, ViewKind};

    fn make_text(s: &str) -> View {
        View::new(
            0,
            ViewKind::Text {
                text: s.to_string(),
                color: Color::WHITE,
                font_size: 16.0,
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
            },
        )
    }

    fn make_box() -> View {
        View::new(0, ViewKind::Box)
    }

    fn make_column() -> View {
        View::new(0, ViewKind::Column)
    }

    #[test]
    fn test_deep_tree() {
        let mut tree = ViewTree::new();

        // Create a deep tree
        let mut current = make_text("Leaf");
        for _ in 0..10 {
            current = make_box().with_children(vec![current]);
        }

        tree.update(&current);
        assert_eq!(tree.len(), 11);
    }

    #[test]
    fn test_wide_tree() {
        let mut tree = ViewTree::new();

        let children: Vec<View> = (0..100)
            .map(|i| make_text(&format!("Item {}", i)))
            .collect();

        let root = make_column().with_children(children);
        tree.update(&root);

        assert_eq!(tree.len(), 101); // 1 column + 100 text
    }

    #[test]
    fn test_incremental_add_child() {
        let mut tree = ViewTree::new();

        // Start with 2 children
        let root1 = make_column().with_children(vec![
            make_text("A").modifier(Modifier::new().key(1)),
            make_text("B").modifier(Modifier::new().key(2)),
        ]);
        tree.update(&root1);
        assert_eq!(tree.len(), 3);

        // Add a third child
        let root2 = make_column().with_children(vec![
            make_text("A").modifier(Modifier::new().key(1)),
            make_text("B").modifier(Modifier::new().key(2)),
            make_text("C").modifier(Modifier::new().key(3)),
        ]);
        tree.update(&root2);
        assert_eq!(tree.len(), 4);
        assert_eq!(tree.stats.created_nodes, 1); // Only C was created
    }

    #[test]
    fn test_incremental_remove_child() {
        let mut tree = ViewTree::new();

        // Start with 3 children
        let root1 = make_column().with_children(vec![
            make_text("A").modifier(Modifier::new().key(1)),
            make_text("B").modifier(Modifier::new().key(2)),
            make_text("C").modifier(Modifier::new().key(3)),
        ]);
        tree.update(&root1);
        assert_eq!(tree.len(), 4);

        // Remove middle child
        let root2 = make_column().with_children(vec![
            make_text("A").modifier(Modifier::new().key(1)),
            make_text("C").modifier(Modifier::new().key(3)),
        ]);
        tree.update(&root2);
        assert_eq!(tree.len(), 3);
        assert_eq!(tree.stats.removed_nodes, 1); // B was removed
    }

    #[test]
    fn test_dirty_propagation() {
        let mut tree = ViewTree::new();

        let root =
            make_column().with_children(vec![make_box().with_children(vec![make_text("Deep")])]);
        tree.update(&root);
        tree.clear_dirty();

        // Change the deep text
        let root2 =
            make_column().with_children(vec![make_box().with_children(vec![make_text("Changed")])]);
        tree.update(&root2);

        // All ancestors should be dirty
        assert!(tree.dirty_nodes().len() >= 3);
    }

    #[test]
    fn test_layout_cache_invalidation() {
        let mut tree = ViewTree::new();

        let root = make_text("Hello");
        let root_id = tree.update(&root);

        // Set layout cache
        tree.set_layout(
            root_id,
            repose_core::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 20.0,
            },
            repose_core::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 20.0,
            },
            crate::LayoutConstraints::default(),
        );

        assert!(tree.get(root_id).unwrap().has_valid_layout());

        // Change content
        let root2 = make_text("Changed");
        tree.update(&root2);

        // Cache should be invalidated
        assert!(!tree.get(root_id).unwrap().has_valid_layout());
    }
}
