//! `nestedMixedLists.spec.ts` — 4 cases.
//!
//! marktext #4341. A list whose type differs from its enclosing list item — a
//! `ul` inside an `ol`, or an `ol` inside a `ul` — was rewritten into a
//! paragraph by the legacy lexer, losing the nested structure. The failure
//! mode to guard against is precisely *"the nested list collapsed into a
//! paragraph"*, which is why two of the four assert the item's children are
//! exactly `[paragraph, <list>]` rather than merely that a list is present.
//!
//! The round-trip cases assert identity **and** a stable second pass, because
//! #4341's damage was progressive across save/reopen cycles.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "nested_mixed_lists",

fn preserves_a_bullet_list_nested_inside_an_ordered_list_item_round_trip() {
    let md = "1. Eat a carrot.\n2. Find an application:\n   - New\n   - Open\n   - Save\n";
    let once = round_trip(md, NESTED_LISTS);
    assert_eq!(once, md);
    assert_eq!(round_trip(&once, NESTED_LISTS), once);
}

fn preserves_an_ordered_list_nested_inside_a_bullet_list_item_round_trip() {
    let md = "- Outer bullet\n- Container item:\n  1. First step\n  2. Second step\n  3. Third step\n";
    let once = round_trip(md, NESTED_LISTS);
    assert_eq!(once, md);
    assert_eq!(round_trip(&once, NESTED_LISTS), once);
}

fn produces_a_bullet_list_state_nested_inside_the_second_order_list_item() {
    let doc = parse(
        "1. Eat a carrot.\n2. Find an application:\n   - New\n   - Open\n   - Save\n",
        NESTED_LISTS,
    );
    let ol = top_named(&doc, "order-list").expect("a top-level order-list state");
    let second = kids(&doc, ol)[1];
    assert_eq!(name(&doc, second), "list-item");

    let nested = child_named(&doc, second, "bullet-list")
        .expect("a bullet-list nested inside the second order-list item");
    // Exactly [leading paragraph, nested bullet-list] — the nested list did
    // NOT collapse into a paragraph, which was #4341's failure mode.
    assert_eq!(names(&doc, &kids(&doc, second)), ["paragraph", "bullet-list"]);
    let items = kids(&doc, nested);
    assert_eq!(items.len(), 3);
    assert_eq!(
        items.iter().map(|i| first_text(&doc, *i)).collect::<Vec<_>>(),
        vec![
            Some("New".to_string()),
            Some("Open".to_string()),
            Some("Save".to_string())
        ]
    );
}

fn produces_an_order_list_state_nested_inside_a_bullet_list_item() {
    let doc = parse(
        "- Outer bullet\n- Container item:\n  1. First step\n  2. Second step\n  3. Third step\n",
        NESTED_LISTS,
    );
    let ul = top_named(&doc, "bullet-list").expect("a top-level bullet-list state");
    let second = kids(&doc, ul)[1];
    assert_eq!(name(&doc, second), "list-item");

    let nested = child_named(&doc, second, "order-list")
        .expect("an order-list nested inside the second bullet-list item");
    assert_eq!(names(&doc, &kids(&doc, second)), ["paragraph", "order-list"]);
    let items = kids(&doc, nested);
    assert_eq!(items.len(), 3);
    assert_eq!(
        items.iter().map(|i| first_text(&doc, *i)).collect::<Vec<_>>(),
        vec![
            Some("First step".to_string()),
            Some("Second step".to_string()),
            Some("Third step".to_string())
        ]
    );
}

}
