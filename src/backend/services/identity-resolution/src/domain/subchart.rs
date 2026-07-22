//! Subchart response DTOs + flat→tree assembly (#348 / #344).
//!
//! Mirrors the .NET `SubchartNodeResponse` / `SubchartResponse` /
//! `SubchartForestResponse` contracts and the `SubchartService.BuildTree`
//! assembly. The repo returns a flat row set; here we index children by parent
//! and recurse from every root (a root arrives with `parent_person_id == None`,
//! both for the single-root anchor and each forest top). Null attribute fields
//! are emitted as JSON `null` so consumers distinguish "no observation" from
//! "missing key".

use std::collections::HashMap;

use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::infra::db::subchart_repo::SubchartFlatNode;

/// One node in the org subchart tree.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubchartNode {
    pub person_id: Uuid,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub status: Option<String>,
    pub subordinates: Vec<SubchartNode>,
}

/// `{ "root": { … } }` — single-root wrapper (locked by the #348 acceptance
/// criteria so the response can gain sibling fields without breaking clients).
#[derive(Debug, Serialize, ToSchema)]
pub struct SubchartResponse {
    pub root: SubchartNode,
}
impl toolkit::api::api_dto::ResponseApiDto for SubchartResponse {}

/// `{ "roots": [ … ] }` — forest wrapper (#344). Empty when the caller has no
/// visible-in-source membership.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubchartForestResponse {
    pub roots: Vec<SubchartNode>,
}
impl toolkit::api::api_dto::ResponseApiDto for SubchartForestResponse {}

/// Assemble flat rows into a forest of trees. Rows with `parent_person_id ==
/// None` are the roots; everything else is indexed by parent and attached
/// recursively. O(N): `org_chart` is a tree (single current parent per child),
/// so each row is consumed exactly once. Ported from `SubchartService.BuildTree`.
#[must_use]
pub fn assemble_forest(flat: Vec<SubchartFlatNode>) -> Vec<SubchartNode> {
    let mut roots: Vec<SubchartFlatNode> = Vec::new();
    let mut by_parent: HashMap<Uuid, Vec<SubchartFlatNode>> = HashMap::new();
    for row in flat {
        match row.parent_person_id {
            None => roots.push(row),
            Some(parent) => by_parent.entry(parent).or_default().push(row),
        }
    }
    roots
        .into_iter()
        .map(|r| build_tree(r, &mut by_parent))
        .collect()
}

fn build_tree(
    node: SubchartFlatNode,
    by_parent: &mut HashMap<Uuid, Vec<SubchartFlatNode>>,
) -> SubchartNode {
    let children = by_parent.remove(&node.person_id).unwrap_or_default();
    let subordinates = children
        .into_iter()
        .map(|c| build_tree(c, by_parent))
        .collect();
    SubchartNode {
        person_id: node.person_id,
        email: node.email,
        display_name: node.display_name,
        job_title: node.job_title,
        status: node.status,
        subordinates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u128, parent: Option<u128>) -> SubchartFlatNode {
        SubchartFlatNode {
            person_id: Uuid::from_u128(id),
            parent_person_id: parent.map(Uuid::from_u128),
            email: None,
            display_name: Some(format!("p{id}")),
            job_title: None,
            status: None,
        }
    }

    #[test]
    fn builds_single_tree_from_flat_rows() -> anyhow::Result<()> {
        use anyhow::Context;
        // root(1) → [2, 3]; 2 → [4]
        let flat = vec![
            node(1, None),
            node(2, Some(1)),
            node(3, Some(1)),
            node(4, Some(2)),
        ];
        let mut forest = assemble_forest(flat);
        assert_eq!(forest.len(), 1);
        let root = forest.remove(0);
        assert_eq!(root.person_id, Uuid::from_u128(1));
        assert_eq!(root.subordinates.len(), 2);
        let two = root
            .subordinates
            .iter()
            .find(|n| n.person_id == Uuid::from_u128(2))
            .context("node 2 present")?;
        assert_eq!(two.subordinates.len(), 1);
        assert_eq!(two.subordinates[0].person_id, Uuid::from_u128(4));
        Ok(())
    }

    #[test]
    fn builds_forest_with_multiple_roots() {
        let flat = vec![node(1, None), node(2, None), node(3, Some(1))];
        let forest = assemble_forest(flat);
        assert_eq!(forest.len(), 2, "two roots");
    }

    #[test]
    fn empty_flat_yields_empty_forest() {
        assert!(assemble_forest(vec![]).is_empty());
    }

    #[test]
    fn leaf_has_empty_subordinates() {
        let forest = assemble_forest(vec![node(1, None)]);
        assert_eq!(forest.len(), 1);
        assert!(forest[0].subordinates.is_empty());
    }
}
