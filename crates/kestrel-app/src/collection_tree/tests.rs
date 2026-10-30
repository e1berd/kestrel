use std::hint::black_box;
use std::time::Instant;

use kestrel_core::format::{FolderFile, Info, RequestFile};
use kestrel_core::{Document, Folder, Request};

use super::*;

fn request(path: &str, name: &str, url: &str) -> Item {
    Item::Request(Request { doc: Document::new(path.into(), RequestFile::new_http(name, "GET", url), true) })
}

fn folder(path: &str, name: &str, children: Vec<Item>) -> Item {
    Item::Folder(Folder {
        dir: path.into(),
        doc: Document::new(
            Path::new(path).join("folder.yml"),
            FolderFile { info: Info { name: name.into(), ..Default::default() }, ..Default::default() },
            true,
        ),
        children,
    })
}

fn fixture() -> Vec<Item> {
    vec![
        folder(
            "/a",
            "Alpha",
            vec![folder("/a/b", "Beta", vec![request("/a/b/one.yml", "Ёжик", "{{baseUrl}}/needle")]), request("/a/two.yml", "Two", "/two")],
        ),
        folder("/empty", "Empty", vec![]),
        request("/three.yml", "Three", "/three"),
    ]
}

fn names(tree: &SidebarTree) -> Vec<&str> {
    tree.rows.iter().map(|&node| tree.index.nodes[node].name.as_str()).collect()
}

#[test]
fn collapsed_folders_count_all_descendants_and_keep_empty_folders() {
    let tree = SidebarTree::new(Arc::new(TreeIndex::new(&fixture())), &HashSet::new());
    assert_eq!(names(&tree), ["Alpha", "Empty", "Three"]);
    assert_eq!(tree.node(0).unwrap().count, 2);
    assert_eq!(tree.node(1).unwrap().count, 0);
    assert_eq!(tree.index.nodes[1].count, 1);
    assert!(tree.node(3).is_none());
}

#[test]
fn expanding_and_collapsing_only_replace_the_branch_and_preserve_descendant_expansion() {
    let mut expanded = HashSet::new();
    let mut tree = SidebarTree::new(Arc::new(TreeIndex::new(&fixture())), &expanded);
    let index = tree.index.clone();
    tree.toggle(Path::new("/a"), &mut expanded);
    assert_eq!(names(&tree), ["Alpha", "Beta", "Two", "Empty", "Three"]);
    tree.toggle(Path::new("/a/b"), &mut expanded);
    assert_eq!(names(&tree), ["Alpha", "Beta", "Ёжик", "Two", "Empty", "Three"]);
    tree.toggle(Path::new("/a"), &mut expanded);
    assert_eq!(names(&tree), ["Alpha", "Empty", "Three"]);
    tree.toggle(Path::new("/a"), &mut expanded);
    assert_eq!(names(&tree), ["Alpha", "Beta", "Ёжик", "Two", "Empty", "Three"]);
    assert!(Arc::ptr_eq(&index, &tree.index));
}

#[test]
fn filtering_matches_names_and_urls_and_keeps_ancestors() {
    let mut tree = SidebarTree::new(Arc::new(TreeIndex::new(&fixture())), &HashSet::new());
    for query in ["ЁЖИК", "NEEDLE"] {
        tree.show_matches(tree.index.matching_rows(query));
        assert_eq!(names(&tree), ["Alpha", "Beta", "Ёжик"]);
    }
    tree.show_matches(tree.index.matching_rows("Alpha"));
    assert_eq!(names(&tree), ["Alpha"]);
    tree.show_matches(tree.index.matching_rows("missing"));
    assert!(tree.rows.is_empty());
}

#[test]
fn clearing_search_restores_expansion_without_rebuilding_the_index() {
    let mut expanded = HashSet::from([PathBuf::from("/a")]);
    let mut tree = SidebarTree::new(Arc::new(TreeIndex::new(&fixture())), &expanded);
    let index = tree.index.clone();
    tree.show_matches(tree.index.matching_rows("needle"));
    tree.toggle(Path::new("/a/b"), &mut expanded);
    assert_eq!(names(&tree), ["Alpha", "Beta", "Ёжик"]);
    tree.show_expanded(&expanded);
    assert_eq!(names(&tree), ["Alpha", "Beta", "Ёжик", "Two", "Empty", "Three"]);
    assert!(Arc::ptr_eq(&index, &tree.index));
}

fn large_fixture() -> Vec<Item> {
    (0..100)
        .map(|group| {
            let path = format!("/group-{group}");
            folder(
                &path,
                &format!("Group {group}"),
                (0..1000)
                    .map(|number| {
                        request(
                            &format!("{path}/{number}.yml"),
                            &format!("Request {group}-{number}"),
                            &format!("/requests/{group}/{number}"),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn a_hundred_thousand_requests_keep_order_counts_search_and_expansion() {
    let items = large_fixture();
    let mut expanded = HashSet::new();
    let mut tree = SidebarTree::new(Arc::new(TreeIndex::new(&items)), &expanded);
    assert_eq!(tree.index.nodes.len(), 100_100);
    assert_eq!(tree.rows.len(), 100);
    assert!(tree.rows.iter().all(|&node| tree.index.nodes[node].count == 1000));
    tree.toggle(Path::new("/group-99"), &mut expanded);
    assert_eq!(tree.rows.len(), 1100);
    assert_eq!(tree.node(1099).unwrap().name, "Request 99-999");
    tree.show_matches(tree.index.matching_rows("/requests/99/999"));
    assert_eq!(names(&tree), ["Group 99", "Request 99-999"]);
    tree.show_expanded(&expanded);
    tree.toggle(Path::new("/group-99"), &mut expanded);
    assert_eq!(tree.rows.len(), 100);
}

#[test]
fn a_hundred_thousand_sibling_folders_remain_individually_addressable() {
    let items: Vec<_> = (0..100_000).map(|number| folder(&format!("/folder-{number}"), &format!("Folder {number}"), vec![])).collect();
    let mut expanded = HashSet::new();
    let mut tree = SidebarTree::new(Arc::new(TreeIndex::new(&items)), &expanded);
    assert_eq!(tree.rows.len(), 100_000);
    tree.toggle(Path::new("/folder-99999"), &mut expanded);
    assert!(expanded.contains(Path::new("/folder-99999")));
    assert_eq!(tree.node(99_999).unwrap().name, "Folder 99999");
    tree.show_matches(tree.index.matching_rows("Folder 99999"));
    assert_eq!(names(&tree), ["Folder 99999"]);
}

#[test]
fn deeply_nested_folders_propagate_counts_and_matches_without_recursive_traversal() {
    let mut items = vec![request("/leaf.yml", "Leaf", "/needle")];
    for depth in (0..512).rev() {
        items = vec![folder(&format!("/depth-{depth}"), &format!("Depth {depth}"), items)];
    }
    let index = TreeIndex::new(&items);
    assert_eq!(index.nodes.len(), 513);
    assert_eq!(index.nodes.last().unwrap().depth, 512);
    assert!(index.nodes.iter().all(|node| node.count == 1));
    assert_eq!(index.matching_rows("needle"), (0..513).collect::<Vec<_>>());
}

#[test]
#[ignore = "manual sidebar benchmark with 100,000 requests"]
fn benchmark_large_sidebar() {
    let items = large_fixture();
    let expanded = (0..100).map(|group| PathBuf::from(format!("/group-{group}"))).collect();
    let started = Instant::now();
    let tree = SidebarTree::new(Arc::new(TreeIndex::new(&items)), &expanded);
    let built = started.elapsed();
    let started = Instant::now();
    for frame in 0..100 {
        let start = frame % (tree.rows.len() - 40);
        for row in start..start + 40 {
            let node = tree.node(row).unwrap();
            black_box((node.path.to_path_buf(), node.name.clone(), node.method.clone(), node.count));
        }
    }
    let viewport = started.elapsed();
    let started = Instant::now();
    for _ in 0..100 {
        black_box(
            tree.index
                .nodes
                .iter()
                .map(|node| (node.path.to_path_buf(), node.name.clone(), node.method.clone(), node.count))
                .collect::<Vec<_>>(),
        );
    }
    let full_tree = started.elapsed();
    let started = Instant::now();
    let matches = tree.index.matching_rows("/requests/99/999");
    let filtered = started.elapsed();
    assert_eq!(matches.len(), 2);
    eprintln!(
        "100,100 nodes: index {built:?}; search {filtered:?}; 100 preparations: full tree {full_tree:?}, viewport of 40 rows {viewport:?}"
    );
}
